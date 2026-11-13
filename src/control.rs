//! Unix domain socket control interface.
//!
//! Lets out-of-process tools (like `ironmud-admin`) push commands into the
//! running server — broadcasting messages, and in the future anything else
//! that needs live-server state. Each client connection carries a single
//! JSON request line and receives a single JSON response line.

use crate::{SharedConnections, broadcast_to_all_players};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ControlCommand {
    Broadcast { message: String },
    Ping,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControlResponse {
    Ok,
    Error { message: String },
}

/// Default socket path: sits next to the database file. Keeping it in the
/// same directory means both the server and the admin wrapper can derive
/// the same path from `IRONMUD_DATABASE` without an extra env var.
pub fn default_socket_path(database_path: &str) -> PathBuf {
    let db = Path::new(database_path);
    let dir = db
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join("control.sock")
}

/// Longest command line accepted from a control client.
const MAX_COMMAND_BYTES: u64 = 64 * 1024;
/// How long a control client has to send its command.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub async fn run_control_socket(path: PathBuf, connections: SharedConnections) -> Result<()> {
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            warn!("Failed to remove stale control socket at {}: {}", path.display(), e);
        }
    }

    let listener =
        UnixListener::bind(&path).with_context(|| format!("Failed to bind control socket at {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)) {
            warn!("Failed to set permissions on control socket: {}", e);
        }
    }

    info!("Control socket listening at {}", path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let conns = connections.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, conns).await {
                        debug!("Control client error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Control socket accept error: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_client(stream: UnixStream, connections: SharedConnections) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    // One command per connection: bound its size and how long we wait for it.
    let mut reader = BufReader::new(read_half.take(MAX_COMMAND_BYTES));
    let mut line = String::new();
    let bytes = match tokio::time::timeout(READ_TIMEOUT, reader.read_line(&mut line)).await {
        Ok(r) => r?,
        Err(_) => anyhow::bail!("control client sent nothing within {:?}", READ_TIMEOUT),
    };
    if bytes == 0 {
        return Ok(());
    }
    let trimmed = line.trim();

    let response = match serde_json::from_str::<ControlCommand>(trimmed) {
        Ok(cmd) => execute(cmd, &connections),
        Err(e) => ControlResponse::Error {
            message: format!("invalid command: {}", e),
        },
    };

    let mut out = serde_json::to_string(&response)?;
    out.push('\n');
    write_half.write_all(out.as_bytes()).await?;
    let _ = write_half.shutdown().await;
    Ok(())
}

fn execute(cmd: ControlCommand, connections: &SharedConnections) -> ControlResponse {
    match cmd {
        ControlCommand::Broadcast { message } => {
            let mut msg = String::from("\n");
            msg.push_str(&message);
            if !msg.ends_with('\n') {
                msg.push('\n');
            }
            broadcast_to_all_players(connections, &msg, None);
            info!("Control: broadcast delivered ({} bytes)", msg.len());
            ControlResponse::Ok
        }
        ControlCommand::Ping => ControlResponse::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn an_oversized_command_is_cut_off_not_buffered() {
        let (server, mut client) = UnixStream::pair().unwrap();
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let task = tokio::spawn(handle_client(server, conns));
        let junk = vec![b'x'; 100 * 1024];
        let _ = client.write_all(&junk).await;
        let mut resp = String::new();
        let _ = BufReader::new(&mut client).read_line(&mut resp).await;
        assert!(resp.contains("invalid command"), "{resp}");
        task.await.unwrap().unwrap();
    }
}
