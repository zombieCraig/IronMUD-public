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
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).await?;
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
            broadcast_to_all_players(connections, &msg);
            info!("Control: broadcast delivered ({} bytes)", msg.len());
            ControlResponse::Ok
        }
        ControlCommand::Ping => ControlResponse::Ok,
    }
}
