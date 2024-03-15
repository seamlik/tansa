use std::path::PathBuf;
use thiserror::Error;

#[derive(PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    Unix,
}

#[derive(Error, Debug)]
pub enum OperatingSystemDetectError {
    #[error("Error in file system")]
    FileSystem(#[from] std::io::Error),

    #[error("Unknown operating system")]
    UnknownOperatingSystem,
}

pub async fn detect_operating_system() -> Result<OperatingSystem, OperatingSystemDetectError> {
    if is_file(&[r"C:\", "Windows", "System32", "ntoskrnl.exe"]).await? {
        return Ok(OperatingSystem::Windows);
    }
    if is_file(&["/", "usr", "bin", "systemctl"]).await? {
        return Ok(OperatingSystem::Unix);
    }

    Err(OperatingSystemDetectError::UnknownOperatingSystem)
}

async fn is_file(path: &[&'static str]) -> std::io::Result<bool> {
    let path_buf: PathBuf = path.iter().collect();
    Ok(tokio::fs::try_exists(&path_buf).await? && tokio::fs::metadata(&path_buf).await?.is_file())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn can_detect() {
        detect_operating_system().await.expect("OS must be known");
    }
}
