use std::collections::BTreeMap;

use bytes::Bytes;

use reqwest::{IntoUrl, Response, StatusCode, Url};

use serde::Deserialize;

use crate::ContainerError;

const MIME_OCI_IMAGE_INDEX: &str = "application/vnd.docker.distribution.manifest.list.v2+json";

#[derive(Debug)]

pub struct RegistryClient {
    api_url: Url,
    token: Option<String>,
    tag: String,
    client: reqwest::Client,
}

impl RegistryClient {
    pub async fn authenticated<U: IntoUrl>(
        url: U,
        image_name: &str,
        tag: &str,
    ) -> Result<Self, ContainerError> {
        let api_url = url
            .into_url()
            .map_err(|_err| ContainerError::Auth("Invalid base URL"))?
            .join(&format!("/v2/library/{image_name}/"))
            .map_err(|_err| ContainerError::Auth("Cannot join url with manifest"))?;

        let token = query_auth_token(
            api_url
                .join(&format!("manifests/{tag}"))
                .map_err(|_err| ContainerError::Auth("Cannot join url with manifest"))?,
        )
        .await?;

        Ok(Self {
            api_url,
            token,
            tag: tag.to_string(),
            client: reqwest::Client::new(),
        })
    }

    pub async fn list_manifests(&self) -> Result<Vec<Manifest>, ContainerError> {
        #[derive(Deserialize)]

        pub struct ManifestList {
            manifests: Vec<Manifest>,
        }

        let url = self
            .api_url
            .join(&format!("manifests/{}", self.tag))
            .map_err(|_err| ContainerError::Auth("Cannot join url with manifest"))?;

        let res: ManifestList = self
            .client
            .get(url)
            .header("Accept", MIME_OCI_IMAGE_INDEX)
            .bearer_auth(self.token.as_deref().unwrap_or_default())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(res.manifests)
    }

    pub async fn read_image_manifest(
        &self,

        manifest: &Manifest,
    ) -> Result<ImageManifest, ContainerError> {
        let url = self
            .api_url
            .join(&format!("manifests/{}", manifest.content.digest))
            .map_err(|_err| ContainerError::Auth("Cannot join url with manifest"))?;

        let res: ImageManifest = self
            .client
            .get(url)
            .header("Accept", &manifest.content.media_type)
            .bearer_auth(self.token.as_deref().unwrap_or_default())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        res.object_type.validate(&manifest.content.media_type)?;

        Ok(res)
    }

    pub async fn read_blob(
        &self,

        element: &ManifestElement,
    ) -> Result<ImageLayerData, ContainerError> {
        let url = self
            .api_url
            .join(&format!("blobs/{}", element.digest))
            .map_err(|_err| ContainerError::Auth("Cannot join url with blobs"))?;

        let res = self
            .client
            .get(url)
            .header("Accept", &element.media_type)
            .bearer_auth(self.token.as_deref().unwrap_or_default())
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        Ok(ImageLayerData(res))
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]

struct ObjectType {
    schema_version: u8,

    media_type: String,
}

impl ObjectType {
    fn validate(&self, mime_type: &str) -> Result<(), ContainerError> {
        if self.schema_version != 2 {
            return Err(ContainerError::Manifest("Invalid version"));
        }

        if self.media_type != mime_type {
            return Err(ContainerError::Manifest("Invalid media type"));
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]

pub struct Manifest {
    #[serde(flatten)]
    pub content: ManifestElement,
    pub platform: ManifestPlatform,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]

pub struct ManifestPlatform {
    pub architecture: String,
    pub os: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]

pub struct ImageManifest {
    pub config: ManifestElement,
    pub layers: Vec<ManifestElement>,
    #[serde(flatten)]
    object_type: ObjectType,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]

pub struct ManifestElement {
    pub media_type: String,
    pub size: usize,
    pub digest: String,
}

#[derive(Debug)]

pub struct ImageLayerData(pub Bytes);

async fn query_auth_token(url: Url) -> Result<Option<String>, ContainerError> {
    #[derive(Deserialize)]

    struct AuthResponse {
        token: String,
    }

    let index_response = reqwest::get(url).await?;

    match index_response.status() {
        StatusCode::UNAUTHORIZED => {
            let auth_addr = parse_auth_addr(&index_response)?;

            let auth_response: AuthResponse = reqwest::get(auth_addr)
                .await?
                .error_for_status()?
                .json()
                .await?;

            Ok(Some(auth_response.token))
        }

        StatusCode::OK => Ok(None),

        code => Err(ContainerError::UnhandledStatusCode(code)),
    }
}

fn parse_auth_addr(response: &Response) -> Result<String, ContainerError> {
    response
        .headers()
        .get("www-authenticate")
        .and_then(|x| x.to_str().ok())
        .ok_or(ContainerError::Auth("Missing www-authenticate header"))
        .and_then(parse_www_authenticate)
}

pub fn parse_www_authenticate(header: &str) -> Result<String, ContainerError> {
    let mut params = BTreeMap::new();

    if !header.starts_with("Bearer ") {
        return Err(ContainerError::Auth("Invalid auth type"));
    }

    for part in header[7..].split(',') {
        if part.is_empty() {
            continue;
        }

        let mut part_iter = part.splitn(2, '=');

        let param_name = part_iter
            .next()
            .ok_or(ContainerError::Auth("Missing www-auth param name"))?;

        let param_value = part_iter
            .next()
            .ok_or(ContainerError::Auth("Missing www-auth param value"))?;

        params.insert(param_name.trim(), param_value.trim().trim_matches('"'));
    }

    let mut output = params
        .remove("realm")
        .ok_or(ContainerError::Auth("Missing ww-auth realm param"))?
        .to_string();

    for (id, (param_name, param_value)) in params.into_iter().enumerate() {
        if id == 0 {
            output.push('?');
        } else {
            output.push('&');
        }

        output.push_str(param_name);
        output.push('=');
        output.push_str(param_value);
    }

    Ok(output)
}
