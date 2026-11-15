//! PROXY protocol v1 (HAProxy's text header) for connections that arrive
//! through a TLS-terminating proxy.
//!
//! Without it, every player behind the proxy appears as `127.0.0.1`: they
//! share the loopback rate-limit budget, site bans can't target them, and
//! logs lose their address. The proxy prepends one line,
//! `PROXY TCP4 <src> <dst> <sport> <dport>\r\n`, and the server reads the
//! real client address from it.
//!
//! The header is only honoured on a dedicated listener (`--proxy-port`) that
//! requires it on every connection. Anyone who can reach that port can claim
//! any address, so it binds to loopback by default; never expose it.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

/// Longest valid v1 header, including the trailing CRLF (spec: 107 bytes).
pub const MAX_HEADER_LEN: usize = 107;

/// How long a proxied connection has to send its header.
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(5);

/// Parse one v1 header line (without the trailing CRLF).
///
/// Returns `Ok(Some(addr))` for `TCP4`/`TCP6`, `Ok(None)` for `UNKNOWN`
/// (e.g. proxy health checks; the caller keeps the socket's peer address).
pub fn parse_v1(line: &str) -> Result<Option<SocketAddr>, String> {
    let mut parts = line.split(' ');
    if parts.next() != Some("PROXY") {
        return Err("missing PROXY signature".into());
    }
    let proto = parts.next().ok_or("missing protocol")?;
    if proto == "UNKNOWN" {
        return Ok(None);
    }
    let fields: Vec<&str> = parts.collect();
    if fields.len() != 4 {
        return Err(format!("expected 4 address fields, got {}", fields.len()));
    }
    let src: IpAddr = fields[0]
        .parse()
        .map_err(|_| format!("bad source address {:?}", fields[0]))?;
    let _dst: IpAddr = fields[1]
        .parse()
        .map_err(|_| format!("bad destination address {:?}", fields[1]))?;
    let sport: u16 = fields[2]
        .parse()
        .map_err(|_| format!("bad source port {:?}", fields[2]))?;
    let _dport: u16 = fields[3]
        .parse()
        .map_err(|_| format!("bad destination port {:?}", fields[3]))?;
    match (proto, src) {
        // A dual-stack proxy reports IPv4 clients as IPv4-mapped IPv6
        // (`::ffff:203.0.113.7`); unmap them so bans and per-IP limits keyed
        // on the plain IPv4 address still match.
        ("TCP4", IpAddr::V4(_)) | ("TCP6", IpAddr::V6(_)) => Ok(Some(SocketAddr::new(src.to_canonical(), sport))),
        ("TCP4", _) | ("TCP6", _) => Err(format!("{proto} with mismatched address family")),
        _ => Err(format!("unsupported protocol {proto:?}")),
    }
}

/// Read the v1 header from the start of `socket`, consuming exactly the
/// header bytes so the telnet stream that follows is untouched.
pub async fn read_header(socket: &mut TcpStream) -> Result<Option<SocketAddr>, String> {
    tokio::time::timeout(HEADER_TIMEOUT, read_header_inner(socket))
        .await
        .map_err(|_| "timed out waiting for PROXY header".to_string())?
}

async fn read_header_inner(socket: &mut TcpStream) -> Result<Option<SocketAddr>, String> {
    let mut buf = Vec::with_capacity(MAX_HEADER_LEN);
    // Byte at a time: the header is short, and over-reading would swallow
    // the client's first telnet bytes.
    loop {
        let b = socket.read_u8().await.map_err(|e| format!("read failed: {e}"))?;
        buf.push(b);
        if buf.ends_with(b"\r\n") {
            break;
        }
        if buf.len() >= MAX_HEADER_LEN {
            return Err("PROXY header too long".into());
        }
    }
    buf.truncate(buf.len() - 2);
    let line = std::str::from_utf8(&buf).map_err(|_| "PROXY header is not ASCII".to_string())?;
    parse_v1(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp4() {
        let a = parse_v1("PROXY TCP4 203.0.113.7 10.0.0.2 51234 4443").unwrap().unwrap();
        assert_eq!(a, "203.0.113.7:51234".parse().unwrap());
    }

    #[test]
    fn parses_tcp6() {
        let a = parse_v1("PROXY TCP6 2001:db8::1 2001:db8::2 40000 4443")
            .unwrap()
            .unwrap();
        assert_eq!(a, "[2001:db8::1]:40000".parse().unwrap());
    }

    #[test]
    fn unmaps_ipv4_mapped_ipv6() {
        let a = parse_v1("PROXY TCP6 ::ffff:203.0.113.7 ::ffff:10.0.0.2 51234 4443")
            .unwrap()
            .unwrap();
        assert_eq!(a, "203.0.113.7:51234".parse().unwrap());
    }

    #[test]
    fn unknown_keeps_peer() {
        assert_eq!(parse_v1("PROXY UNKNOWN").unwrap(), None);
        assert_eq!(parse_v1("PROXY UNKNOWN ffff:f::1 ffff:f::2 1 2").unwrap(), None);
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "GET / HTTP/1.1",
            "PROXY",
            "PROXY TCP4 1.2.3.4 5.6.7.8 1",
            "PROXY TCP4 not-an-ip 5.6.7.8 1 2",
            "PROXY TCP4 1.2.3.4 5.6.7.8 99999 2",
            "PROXY TCP4 2001:db8::1 5.6.7.8 1 2",
            "PROXY TCP6 1.2.3.4 5.6.7.8 1 2",
            "PROXY UDP4 1.2.3.4 5.6.7.8 1 2",
        ] {
            assert!(parse_v1(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[tokio::test]
    async fn read_header_leaves_following_bytes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(b"PROXY TCP4 198.51.100.9 127.0.0.1 6000 4010\r\nhello")
                .await
                .unwrap();
            s
        });
        let (mut server, _) = listener.accept().await.unwrap();
        let addr = read_header(&mut server).await.unwrap().unwrap();
        assert_eq!(addr, "198.51.100.9:6000".parse().unwrap());
        let mut rest = [0u8; 5];
        server.read_exact(&mut rest).await.unwrap();
        assert_eq!(&rest, b"hello");
        drop(client.await.unwrap());
    }

    #[tokio::test]
    async fn read_header_rejects_overlong() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(&[b'A'; 200]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let (mut server, _) = listener.accept().await.unwrap();
        assert!(read_header(&mut server).await.is_err());
    }
}
