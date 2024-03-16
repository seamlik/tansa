use std::process::Stdio;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::process::Command;

pub async fn eval(command: &str, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>, ProcessError> {
    let mut process = Command::new(command)
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin_pipe = process
        .stdin
        .take()
        .ok_or_else(|| ProcessError::RedirectStdIo)?;
    stdin_pipe.write_all(stdin).await?;
    drop(stdin_pipe);

    if !process.wait().await?.success() {
        return Err(ProcessError::ExternalCommand);
    }

    read_stdout(&mut process).await
}

pub async fn run(command: &str, args: &[&str]) -> Result<Vec<u8>, ProcessError> {
    let mut process = Command::new(command)
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if !process.wait().await?.success() {
        return Err(ProcessError::ExternalCommand);
    }

    read_stdout(&mut process).await
}

pub async fn probe(command: &str, args: &[&str]) -> bool {
    let mut process = if let Ok(p) = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
    {
        p
    } else {
        log::debug!("Failed to spawn process of command {}", command);
        return false;
    };

    let status = if let Ok(s) = process.wait().await {
        s
    } else {
        log::debug!("Failed to wait for process of command {}", command);
        return false;
    };

    if status.success() {
        true
    } else {
        log::debug!("Command {} exited with failure", command);
        false
    }
}

async fn read_stdout(process: &mut Child) -> Result<Vec<u8>, ProcessError> {
    let mut stdout = process
        .stdout
        .take()
        .ok_or_else(|| ProcessError::RedirectStdIo)?;

    let mut buffer = Default::default();
    stdout.read_to_end(&mut buffer).await?;
    Ok(buffer)
}

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("Failed in create a child process")]
    ChildProcessCreation(#[from] std::io::Error),

    #[error("Failed to redirect standard I/O")]
    RedirectStdIo,

    #[error("External command failed")]
    ExternalCommand,
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn eval() {
        crate::test::init();

        let output = super::eval("pwsh", &["-Command", "-"], b"Write-Output 123")
            .await
            .unwrap();
        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(output_text.trim(), "123");
    }

    #[tokio::test]
    async fn eval_failure() {
        crate::test::init();

        let e = super::eval("pwsh", &["-Command", "-"], b"Some-Unknown-Command")
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "External command failed");
    }

    #[tokio::test]
    async fn run() {
        crate::test::init();

        let output = super::run("pwsh", &["-Command", "Write-Output 123"])
            .await
            .unwrap();
        let output_text = String::from_utf8(output).unwrap();
        assert_eq!(output_text.trim(), "123");
    }

    #[tokio::test]
    async fn run_failure() {
        crate::test::init();

        let e = super::run("pwsh", &["-Command", "Some-Unknown-Command"])
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "External command failed");
    }

    #[tokio::test]
    async fn probe() {
        crate::test::init();

        assert!(super::probe("pwsh", &["-Help"]).await);
    }

    #[tokio::test]
    async fn probe_non_existing_command() {
        crate::test::init();

        assert!(!super::probe("__some_unknown_command__", &["--version"]).await);
    }

    #[tokio::test]
    async fn probe_invalid_argument() {
        crate::test::init();

        assert!(!super::probe("pwsh", &["--some-invalid-argument"]).await);
    }
}
