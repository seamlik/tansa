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

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("Failed in create a child process")]
    ChildProcessCreation(#[from] std::io::Error),

    #[error("Failed to redirect standard I/O")]
    StdioRedirection,

    #[error("External command failed")]
    ExternalCommand,
}
