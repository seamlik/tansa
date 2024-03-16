use std::process::Stdio;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub async fn eval(command: &str, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>, ProcessError> {
    let mut process = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin_pipe = process
        .stdin
        .take()
        .ok_or_else(|| ProcessError::StdioRedirection)?;
    stdin_pipe.write_all(stdin).await?;
    drop(stdin_pipe);

    if !process.wait().await?.success() {
        return Err(ProcessError::ExternalCommand);
    }

    let mut stdout = process
        .stdout
        .take()
        .ok_or_else(|| ProcessError::StdioRedirection)?;

    let mut buffer = Default::default();
    stdout.read_to_end(&mut buffer).await?;
    Ok(buffer)
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

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("Failed in create a child process")]
    ChildProcessCreation(#[from] std::io::Error),

    #[error("Failed to redirect standard I/O")]
    StdioRedirection,

    #[error("External command failed")]
    ExternalCommand,
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn probe() {
        assert!(super::probe("pwsh", &["-Help"]).await);
    }
}
