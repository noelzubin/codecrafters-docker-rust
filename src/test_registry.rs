use docker_starter_rust::{
    registry::{parse_www_authenticate, RegistryClient},
    ContainerError,
};

const URL: &str = "https://registry.hub.docker.com";

#[tokio::test]
async fn test_auth() {
    assert!(RegistryClient::authenticated(URL, "ubuntu", "latest")
        .await
        .is_ok());
}

#[test]
fn parse_parse_www_authenticate_invalid() {
    assert_eq!(
        parse_www_authenticate(""),
        Err(ContainerError::Auth("Invalid auth type"))
    );

    assert_eq!(
        parse_www_authenticate("Bearer "),
        Err(ContainerError::Auth("Missing ww-auth realm param"))
    );

    assert_eq!(
        parse_www_authenticate("Bearer realm"),
        Err(ContainerError::Auth("Missing www-auth param value"))
    );
}

#[test]

fn parse_parse_www_authenticate_valid() {
    assert_eq!(
        parse_www_authenticate("Bearer realm=hello").as_deref(),
        Ok("hello")
    );

    assert_eq!(
        parse_www_authenticate(
            r#"Bearer realm=https://auth.docker.io/token,service="registry.docker.io""#
        )
        .as_deref(),
        Ok("https://auth.docker.io/token?service=registry.docker.io")
    );

    assert_eq!(
        parse_www_authenticate(
            r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io""#
        )
        .as_deref(),
        Ok("https://auth.docker.io/token?service=registry.docker.io")
    );

    assert_eq!(
        parse_www_authenticate(
            r#"Bearer service="registry.docker.io",realm="https://auth.docker.io/token""#
        )
        .as_deref(),
        Ok("https://auth.docker.io/token?service=registry.docker.io")
    );

    assert_eq!(

        parse_www_authenticate(

            r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:samalba/my-app:pull""#

        )

        .as_deref(),

        Ok("https://auth.docker.io/token?scope=repository:samalba/my-app:pull&service=registry.docker.io")

    );
}
