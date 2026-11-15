//! The PROXY-protocol listener admits connections by the client address in
//! the header, not the proxy's loopback address.

use ironmud::db::Db;
use ironmud::types::SiteBanRecord;
use ironmud::{World, run_server_with_proxy};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

struct Harness {
    proxy_port: u16,
    _shutdown: tokio::sync::oneshot::Sender<()>,
    _tmp: tempfile::TempDir,
}

async fn start(banned_ip: &str) -> Harness {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.put_site_ban(&SiteBanRecord {
        ip: banned_ip.to_string(),
        reason: "test".into(),
        banned_by: "test".into(),
        banned_at: 0,
        expires_at: None,
    })
    .unwrap();
    let connections = Arc::new(Mutex::new(HashMap::new()));
    let state = Arc::new(Mutex::new(World::minimal(db, connections)));

    let direct = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = proxy.local_addr().unwrap().port();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(run_server_with_proxy(state, direct, Some(proxy), rx));
    Harness {
        proxy_port,
        _shutdown: tx,
        _tmp: tmp,
    }
}

async fn read_all(stream: &mut TcpStream) -> String {
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn site_ban_applies_to_the_header_address() {
    let h = start("198.51.100.9").await;
    let mut s = TcpStream::connect(("127.0.0.1", h.proxy_port)).await.unwrap();
    s.write_all(b"PROXY TCP4 198.51.100.9 127.0.0.1 5555 4010\r\n")
        .await
        .unwrap();
    let reply = read_all(&mut s).await;
    assert!(
        reply.contains("not permitted"),
        "banned client behind the proxy was not refused: {reply:?}"
    );
}

#[tokio::test]
async fn connection_without_header_is_dropped() {
    let h = start("198.51.100.9").await;
    let mut s = TcpStream::connect(("127.0.0.1", h.proxy_port)).await.unwrap();
    s.write_all(b"hello there\r\n").await.unwrap();
    let reply = read_all(&mut s).await;
    assert!(
        reply.is_empty(),
        "headerless connection should be closed silently: {reply:?}"
    );
}

#[tokio::test]
async fn site_ban_matches_ipv4_mapped_header_address() {
    // HAProxy on a dual-stack bind reports IPv4 clients as ::ffff:a.b.c.d.
    let h = start("198.51.100.9").await;
    let mut s = TcpStream::connect(("127.0.0.1", h.proxy_port)).await.unwrap();
    s.write_all(b"PROXY TCP6 ::ffff:198.51.100.9 ::ffff:127.0.0.1 5555 4010\r\n")
        .await
        .unwrap();
    let reply = read_all(&mut s).await;
    assert!(
        reply.contains("not permitted"),
        "mapped address escaped the ban: {reply:?}"
    );
}
