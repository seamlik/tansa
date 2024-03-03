use anyhow::Context;
use std::process::Stdio;
use std::str::FromStr;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub async fn serve(service_port: u16) -> anyhow::Result<()> {
    log::info!("Getting multicast interface indexes with JShell");
    let multicast_interface_indexes = get_multicast_interface_indexes().await?;
    log::info!(
        "Multicast interface indexes: {:?}",
        &multicast_interface_indexes
    );
    let multicast_address = crate::MULTICAST_ADDRESS.parse()?;
    tansa::serve(multicast_address, multicast_interface_indexes, service_port).await?;
    Ok(())
}

async fn get_multicast_interface_indexes() -> anyhow::Result<Vec<u32>> {
    let command = "jshell";
    let mut process = Command::new(command)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to spawn process `{}`", command))?;
    let mut stdin = process
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("STDIN unavailable"))?;
    let script = include_bytes!("GetMulticastInterfaceIndexes.java");
    stdin.write_all(script).await?;
    drop(stdin);
    if !process.wait().await?.success() {
        anyhow::bail!("JShell failed");
    }
    let mut stdout = process
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("STDOUT unavailable"))?;
    let mut result = String::default();
    stdout.read_to_string(&mut result).await?;
    result
        .split_whitespace()
        .map(FromStr::from_str)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn get_multicast_interface_indexes() -> anyhow::Result<()> {
        let indexes = super::get_multicast_interface_indexes().await?;
        assert!(
            !indexes.is_empty(),
            "At least 1 multicast interface (loopback) should exist in all CI environments"
        );
        Ok(())
    }
}
