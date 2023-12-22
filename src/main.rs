use std::{
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};
use libc;

use anyhow::{Context, Result};

use tempfile::TempDir;

fn main() -> Result<()> {
    let env: Vec<_> = std::env::args().collect();
    let args = Args::parse(&env)?;
    let container = Container::new(&args)?;
    let status = container.exec()?;
    std::process::exit(status.code().unwrap_or(1));
}

struct Args<'a> {
    command: &'a str,

    args: &'a [String],
}

impl<'a> Args<'a> {
    fn parse(args: &'a [String]) -> Result<Self> {
        let command = args.get(3).context("Failed to parse command")?;

        let command_args = args.get(4..).context("Failed to parse arguments")?;

        Ok(Self {
            command,

            args: command_args,
        })
    }
}

struct Container<'a> {
    command: PathBuf,
    args: &'a [String],
    root_dir: TempDir,
}

impl<'a> Container<'a> {
    fn new(args: &'a Args) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        std::fs::create_dir_all(temp.path().join("dev"))?;
        std::fs::File::create(temp.path().join("dev/null"))?;
        let command_filename = Path::new(args.command)
            .file_name()
            .context("Invalid command")?;
        std::fs::copy(args.command, temp.path().join(command_filename))?;
        Ok(Self {
            command: Path::new("/").join(command_filename),
            args: args.args,
            root_dir: temp,
        })
    }

    fn exec(&self) -> Result<ExitStatus> {
        // NOTE: Does not compile on macos
        assert_eq!(
            unsafe { libc::unshare(libc::CLONE_NEWPID) },
            0,
            "unshare fail"
        );

        let mut command = std::process::Command::new(&self.command);
        command
            .args(self.args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(Stdio::inherit())
            .env_clear();

        std::os::unix::fs::chroot(self.root_dir.path())?;
        let mut handle = command.spawn()?;
        let status = handle.wait()?;
        Ok(status)
    }
}
