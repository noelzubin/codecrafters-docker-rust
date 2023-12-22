use std::{
    env, fs,
    os::unix::{self, process::CommandExt},
    process::{exit, Command, ExitStatus, Stdio},
};
use anyhow::Context;
use docker_starter_rust::{fs_utils, registry::RegistryClient};
use tempfile::{tempdir, TempDir};
// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let container = Container::new(cli).await?;
    let status = container.exec()?;
    exit(status.code().unwrap_or(1));
}

#[derive(Debug)]
struct Cli {
    image: String,
    tag: String,
    command: String,
    args: Vec<String>,
}

impl Cli {
    fn parse() -> Self {
        let full_image = env::args().nth(2).expect("Missing image args");
        let full_image_items: Vec<_> = full_image.rsplitn(2, ':').collect();
        let (image, tag) = match &full_image_items[..] {
            [tag, image] => (image.to_string(), tag.to_string()),
            [image] => (image.to_string(), "latest".to_string()),
            _ => panic!("Fail to parse image name"),
        };

        let command = env::args().nth(3).expect("Missing command args");
        let args = env::args().skip(4).collect();

        Self {
            image,
            tag,
            command,
            args,
        }
    }
}

#[derive(Debug)]
struct Container {
    command: String,
    args: Vec<String>,
    chroot_dir: TempDir,
}

impl Container {
    async fn new(cli: Cli) -> anyhow::Result<Self> {
        let registry_client =
            RegistryClient::authenticated("https://registry.hub.docker.com", &cli.image, &cli.tag)
                .await?;

        // Download layer from docker hub
        let manifests = registry_client.list_manifests().await?;
        let target_manifest = manifests
            .into_iter()
            .find(|m| m.platform.architecture == "amd64" && m.platform.os == "linux")
            .with_context(|| "No platform found")?;

        let image_manifest = registry_client
            .read_image_manifest(&target_manifest)
            .await?;

        let layer = registry_client.read_blob(&image_manifest.layers[0]).await?;

        // Prepare chroot.
        let chroot_dir =
            tempdir().with_context(|| "Cannot create temporary chroot dir".to_string())?;

        let chroot_path = chroot_dir.path();

        // Create /dev/null in chroot
        fs::create_dir_all(chroot_path.join("dev"))?;
        fs::write(chroot_path.join("dev/null"), "")?;

        // Uncompressed layer to chroot
        fs_utils::decompress_layer(layer, chroot_path)?;

        Ok(Self {
            command: cli.command,
            args: cli.args,
            chroot_dir,
        })

    }

    fn exec(&self) -> anyhow::Result<ExitStatus> {

        // Isolate PID namespace
        // NOTE: Need to be called on parent process.
        assert_eq!(
            unsafe {
                libc::unshare(
                    libc::CLONE_NEWCGROUP
                        | libc::CLONE_NEWIPC
                        | libc::CLONE_NEWNS
                        | libc::CLONE_NEWPID
                        // | libc::CLONE_NEWUSER
                        | libc::CLONE_NEWUTS,
                )
            },
            0,
            "unshare fail"
        );

        // Pipe file descriptor and clean env.
        let mut ps = Command::new(&self.command);

        ps.args(&self.args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .env_clear();

        let chroot_path = self.chroot_dir.path().to_path_buf();
        assert!(chroot_path.exists());

        unsafe {
            ps.pre_exec(move || {
                // Isolate process before spawning it.
                unix::fs::chroot(&chroot_path)?;
                env::set_current_dir("/")?;
                Ok(())
            });

        }

        // Spawn process.
        let mut child = ps.spawn()?;

        // Wait for its completion.
        let status = child.wait()?;

        Ok(status)
    }
}