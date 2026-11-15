//! Operational endpoints for hosting and monitoring: `GET /healthz` and
//! `GET /metrics` (Prometheus text format).
//!
//! These run on their own listener, separate from the REST API: the API is
//! off by default and needs an API key, while uptime checks, container
//! health checks and metric scrapers need an unauthenticated probe. The
//! listener is on by default and binds `127.0.0.1:4002`, so nothing is
//! exposed beyond the host unless an operator opts in.
//!
//! Counters are process-global atomics bumped at existing chokepoints
//! (accept loop, `call_run_command_guarded`, first character attach, tick
//! supervisor). Rendering is a pure function over a [`MetricsSnapshot`] so it
//! can be tested without a server.
//!
//! Lock discipline: a scrape takes the Connections lock only long enough to
//! count sessions and never touches the World lock.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use parking_lot::Mutex;
use serde_json::json;

use crate::SharedConnections;
use crate::db::Db;
use crate::heartbeat::{self, TickStatus};

/// Default port for the ops listener.
pub const DEFAULT_OPS_PORT: u16 = 4002;
/// Default bind address for the ops listener (loopback only).
pub const DEFAULT_OPS_BIND: &str = "127.0.0.1";

/// TCP connections accepted on the game port (before IP-cap / ban checks).
pub static CONNECTIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Characters entering the world (first character attach of a session).
pub static LOGINS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Player command scripts run.
pub static COMMANDS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Player command scripts that panicked in native code (caught).
pub static COMMAND_PANICS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Player command scripts that returned a Rhai error.
pub static SCRIPT_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Add one to a process-global counter.
pub fn inc(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

fn tick_restarts() -> &'static Mutex<HashMap<&'static str, u64>> {
    static R: OnceLock<Mutex<HashMap<&'static str, u64>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record that the tick supervisor restarted `name` after a panic.
pub fn record_tick_restart(name: &'static str) {
    *tick_restarts().lock().entry(name).or_insert(0) += 1;
}

fn tick_restart_counts() -> Vec<(&'static str, u64)> {
    let mut v: Vec<_> = tick_restarts().lock().iter().map(|(k, v)| (*k, *v)).collect();
    v.sort_by_key(|(k, _)| *k);
    v
}

fn broken_scripts() -> &'static Mutex<BTreeSet<String>> {
    static S: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Record the outcome of compiling one Rhai script (startup load or hot
/// reload). A failure is remembered until the same path compiles again, so
/// the gauge counts scripts that are broken *now*.
pub fn record_script_load(path: &str, ok: bool) {
    let mut broken = broken_scripts().lock();
    if ok {
        broken.remove(path);
    } else {
        broken.insert(path.to_string());
    }
}

/// Number of scripts whose latest compile failed.
pub fn script_load_errors() -> u64 {
    broken_scripts().lock().len() as u64
}

fn process_start_time() -> i64 {
    static START: OnceLock<i64> = OnceLock::new();
    *START.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    })
}

/// True when `bind` names a loopback address (or `localhost`).
pub fn is_loopback_bind(bind: &str) -> bool {
    if bind.eq_ignore_ascii_case("localhost") {
        return true;
    }
    bind.trim_matches(|c| c == '[' || c == ']')
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Join a bind host and port into a socket-address string, bracketing bare
/// IPv6 hosts (`::` -> `[::]:4002`) so they parse.
pub fn bind_addr(bind: &str, port: u16) -> String {
    if bind.contains(':') && !bind.starts_with('[') {
        format!("[{bind}]:{port}")
    } else {
        format!("{bind}:{port}")
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Result of a health evaluation over the tick heartbeat registry.
#[derive(Debug, Clone)]
pub struct HealthReport {
    pub tick_count: usize,
    pub stale: Vec<TickStatus>,
}

impl HealthReport {
    pub fn is_healthy(&self) -> bool {
        self.stale.is_empty()
    }

    pub fn to_json(&self) -> serde_json::Value {
        if self.is_healthy() {
            json!({ "status": "ok", "ticks": self.tick_count })
        } else {
            let stale: Vec<_> = self
                .stale
                .iter()
                .map(|s| {
                    json!({
                        "name": s.name,
                        "age_secs": s.age().as_secs(),
                        "expected_interval_secs": s.expected_interval.as_secs(),
                    })
                })
                .collect();
            json!({ "status": "stale", "ticks": self.tick_count, "stale": stale })
        }
    }
}

/// Evaluate health from a heartbeat snapshot.
pub fn health_report(ticks: &[TickStatus]) -> HealthReport {
    HealthReport {
        tick_count: ticks.len(),
        stale: ticks.iter().filter(|t| t.is_stale()).cloned().collect(),
    }
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Everything `/metrics` reports, gathered up front so rendering is pure.
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub version: String,
    pub git_hash: String,
    pub start_time_seconds: i64,
    pub players_online: u64,
    pub sessions_open: u64,
    pub connections_total: u64,
    pub logins_total: u64,
    pub commands_total: u64,
    pub command_panics_total: u64,
    pub script_errors_total: u64,
    pub script_load_errors: u64,
    pub db_size_bytes: Option<u64>,
    pub ticks: Vec<TickStatus>,
    pub tick_restarts: Vec<(&'static str, u64)>,
}

/// Gather a snapshot from process state. Locks Connections briefly; never
/// locks World.
pub fn collect_metrics(db: &Db, connections: &SharedConnections) -> MetricsSnapshot {
    let (players_online, sessions_open) = {
        let conns = connections.lock();
        let players = conns.values().filter(|s| s.character.is_some()).count();
        (players as u64, conns.len() as u64)
    };
    MetricsSnapshot {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_hash: env!("IRONMUD_GIT_HASH").to_string(),
        start_time_seconds: process_start_time(),
        players_online,
        sessions_open,
        connections_total: CONNECTIONS_TOTAL.load(Ordering::Relaxed),
        logins_total: LOGINS_TOTAL.load(Ordering::Relaxed),
        commands_total: COMMANDS_TOTAL.load(Ordering::Relaxed),
        command_panics_total: COMMAND_PANICS_TOTAL.load(Ordering::Relaxed),
        script_errors_total: SCRIPT_ERRORS_TOTAL.load(Ordering::Relaxed),
        script_load_errors: script_load_errors(),
        db_size_bytes: db.size_on_disk().ok(),
        ticks: heartbeat::snapshot(),
        tick_restarts: tick_restart_counts(),
    }
}

/// Escape a Prometheus label value (backslash, double quote, newline).
fn escape_label(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn metric(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
}

/// Render a snapshot in the Prometheus text exposition format (0.0.4).
pub fn render_metrics(m: &MetricsSnapshot) -> String {
    let mut out = String::new();

    metric(&mut out, "ironmud_build_info", "gauge", "Build version and git hash.");
    let _ = writeln!(
        out,
        "ironmud_build_info{{version=\"{}\",git=\"{}\"}} 1",
        escape_label(&m.version),
        escape_label(&m.git_hash)
    );

    metric(
        &mut out,
        "ironmud_start_time_seconds",
        "gauge",
        "Unix time the server process started.",
    );
    let _ = writeln!(out, "ironmud_start_time_seconds {}", m.start_time_seconds);

    metric(
        &mut out,
        "ironmud_players_online",
        "gauge",
        "Sessions with a character in the world.",
    );
    let _ = writeln!(out, "ironmud_players_online {}", m.players_online);

    metric(
        &mut out,
        "ironmud_sessions_open",
        "gauge",
        "Open sessions, including ones still at login.",
    );
    let _ = writeln!(out, "ironmud_sessions_open {}", m.sessions_open);

    let counters: [(&str, &str, u64); 5] = [
        (
            "ironmud_connections_total",
            "TCP connections accepted on the game port.",
            m.connections_total,
        ),
        (
            "ironmud_logins_total",
            "Characters that entered the world.",
            m.logins_total,
        ),
        (
            "ironmud_commands_total",
            "Player command scripts run.",
            m.commands_total,
        ),
        (
            "ironmud_command_panics_total",
            "Player commands that panicked in native code.",
            m.command_panics_total,
        ),
        (
            "ironmud_script_errors_total",
            "Player commands that returned a script error.",
            m.script_errors_total,
        ),
    ];
    for (name, help, value) in counters {
        metric(&mut out, name, "counter", help);
        let _ = writeln!(out, "{name} {value}");
    }

    metric(
        &mut out,
        "ironmud_script_load_errors",
        "gauge",
        "Rhai scripts whose latest compile failed (startup or hot reload); the commands they define don't work.",
    );
    let _ = writeln!(out, "ironmud_script_load_errors {}", m.script_load_errors);

    if let Some(bytes) = m.db_size_bytes {
        metric(&mut out, "ironmud_db_size_bytes", "gauge", "Database size on disk.");
        let _ = writeln!(out, "ironmud_db_size_bytes {bytes}");
    }

    metric(
        &mut out,
        "ironmud_tick_last_beat_age_seconds",
        "gauge",
        "Seconds since each background tick last ran.",
    );
    for t in &m.ticks {
        let _ = writeln!(
            out,
            "ironmud_tick_last_beat_age_seconds{{tick=\"{}\"}} {:.3}",
            escape_label(t.name),
            t.age().as_secs_f64()
        );
    }

    metric(
        &mut out,
        "ironmud_tick_stale",
        "gauge",
        "1 if a background tick has missed its expected cadence.",
    );
    for t in &m.ticks {
        let _ = writeln!(
            out,
            "ironmud_tick_stale{{tick=\"{}\"}} {}",
            escape_label(t.name),
            u8::from(t.is_stale())
        );
    }

    metric(
        &mut out,
        "ironmud_tick_restarts_total",
        "counter",
        "Times the supervisor restarted a tick after a panic.",
    );
    for (name, n) in &m.tick_restarts {
        let _ = writeln!(
            out,
            "ironmud_tick_restarts_total{{tick=\"{}\"}} {}",
            escape_label(name),
            n
        );
    }

    out
}

// ---------------------------------------------------------------------------
// HTTP server
// ---------------------------------------------------------------------------

/// Shared handles for the ops handlers.
pub struct OpsState {
    pub db: Db,
    pub connections: SharedConnections,
}

async fn healthz() -> Response {
    let report = health_report(&heartbeat::snapshot());
    let status = if report.is_healthy() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(report.to_json())).into_response()
}

async fn metrics(State(state): State<Arc<OpsState>>) -> Response {
    let snapshot = collect_metrics(&state.db, &state.connections);
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        render_metrics(&snapshot),
    )
        .into_response()
}

/// Router with `/healthz` and `/metrics`.
pub fn router(state: Arc<OpsState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
        .with_state(state)
}

/// Serve the ops endpoints. A bind failure is logged, not fatal: the game
/// should keep running if the ops port is taken.
pub async fn run_ops_server(state: Arc<OpsState>, bind: &str, port: u16) {
    // Pin the start time at boot rather than at first scrape.
    process_start_time();
    let addr = bind_addr(bind, port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Ops endpoints disabled: could not bind {}: {}", addr, e);
            return;
        }
    };
    tracing::info!("Ops endpoints (/healthz, /metrics) listening on {}", addr);
    if let Err(e) = axum::serve(listener, router(state)).await {
        tracing::error!("Ops server stopped: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Client probe (used by `ironmud-admin health`)
// ---------------------------------------------------------------------------

/// GET `/healthz` from `host:port` with a plain blocking HTTP/1.0 request.
/// Returns the status code and body. No HTTP client dependency, so it works
/// in a minimal container image.
pub fn probe_health(host: &str, port: u16, timeout: Duration) -> std::io::Result<(u16, String)> {
    let addr = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "address did not resolve"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET /healthz HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let status = raw
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed HTTP response"))?;
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn tick(name: &'static str, interval_secs: u64, age_secs: u64) -> TickStatus {
        TickStatus {
            name,
            expected_interval: Duration::from_secs(interval_secs),
            last_beat: Instant::now()
                .checked_sub(Duration::from_secs(age_secs))
                .expect("monotonic clock far enough from boot"),
        }
    }

    #[test]
    fn bind_addr_brackets_ipv6() {
        assert_eq!(bind_addr("127.0.0.1", 4002), "127.0.0.1:4002");
        assert_eq!(bind_addr("::", 4002), "[::]:4002");
        assert_eq!(bind_addr("[::1]", 4002), "[::1]:4002");
        assert_eq!(bind_addr("localhost", 4002), "localhost:4002");
        for a in ["127.0.0.1:4002", "[::]:4002", "[::1]:4002"] {
            assert!(a.parse::<std::net::SocketAddr>().is_ok(), "{a}");
        }
    }

    #[test]
    fn healthy_when_no_tick_is_stale() {
        let r = health_report(&[tick("a", 10, 1), tick("b", 60, 5)]);
        assert!(r.is_healthy());
        assert_eq!(r.to_json()["status"], "ok");
        assert_eq!(r.to_json()["ticks"], 2);
    }

    #[test]
    fn unhealthy_lists_stale_ticks() {
        // 10s interval, 2x multiplier, 30s floor -> stale after 30s.
        let r = health_report(&[tick("fresh", 10, 1), tick("dead", 10, 120)]);
        assert!(!r.is_healthy());
        let j = r.to_json();
        assert_eq!(j["status"], "stale");
        assert_eq!(j["stale"][0]["name"], "dead");
        assert_eq!(j["stale"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn empty_registry_is_healthy() {
        assert!(health_report(&[]).is_healthy());
    }

    #[test]
    fn script_load_errors_track_currently_broken_scripts() {
        let a = "scripts/commands/__ops_test_a.rhai";
        let b = "scripts/commands/__ops_test_b.rhai";
        let base = script_load_errors();
        record_script_load(a, false);
        record_script_load(b, false);
        record_script_load(a, false); // failing again doesn't double-count
        assert_eq!(script_load_errors(), base + 2);
        record_script_load(a, true); // fixed and hot-reloaded
        assert_eq!(script_load_errors(), base + 1);
        record_script_load(b, true);
        assert_eq!(script_load_errors(), base);
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_bind("127.0.0.1"));
        assert!(is_loopback_bind("::1"));
        assert!(is_loopback_bind("[::1]"));
        assert!(is_loopback_bind("localhost"));
        assert!(!is_loopback_bind("0.0.0.0"));
        assert!(!is_loopback_bind("10.0.0.5"));
        assert!(!is_loopback_bind("example.com"));
    }

    #[test]
    fn render_has_type_lines_and_parseable_samples() {
        let m = MetricsSnapshot {
            version: "1.2.3".into(),
            git_hash: "abc\"def".into(),
            start_time_seconds: 1_700_000_000,
            players_online: 3,
            sessions_open: 5,
            connections_total: 10,
            logins_total: 4,
            commands_total: 99,
            command_panics_total: 1,
            script_errors_total: 2,
            script_load_errors: 3,
            db_size_bytes: Some(4096),
            ticks: vec![tick("combat", 1, 0), tick("dead", 10, 120)],
            tick_restarts: vec![("combat", 2)],
        };
        let text = render_metrics(&m);

        // Every sample line is `name[{labels}] value` with a numeric value,
        // and its metric family has a # TYPE line.
        let mut typed = std::collections::HashSet::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("# TYPE ") {
                typed.insert(rest.split_whitespace().next().unwrap().to_string());
                continue;
            }
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let (name_part, value) = line.rsplit_once(' ').expect("sample has a value");
            value
                .parse::<f64>()
                .unwrap_or_else(|_| panic!("non-numeric value in {line:?}"));
            let name = name_part.split('{').next().unwrap();
            assert!(typed.contains(name), "{name} sampled before its # TYPE line");
        }

        assert!(text.contains("ironmud_build_info{version=\"1.2.3\",git=\"abc\\\"def\"} 1"));
        assert!(text.contains("ironmud_players_online 3\n"));
        assert!(text.contains("ironmud_commands_total 99\n"));
        assert!(text.contains("ironmud_db_size_bytes 4096\n"));
        assert!(text.contains("ironmud_script_load_errors 3\n"));
        assert!(text.contains("ironmud_tick_stale{tick=\"dead\"} 1\n"));
        assert!(text.contains("ironmud_tick_stale{tick=\"combat\"} 0\n"));
        assert!(text.contains("ironmud_tick_restarts_total{tick=\"combat\"} 2\n"));
    }

    #[test]
    fn db_size_omitted_when_unknown() {
        let text = render_metrics(&MetricsSnapshot::default());
        assert!(!text.contains("ironmud_db_size_bytes"));
    }
}
