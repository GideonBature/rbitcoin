//! Asmap files an operator drops in the datadir, checked from one node.
//!
//! Inbound clients are raw BIP324 sockets bound on `127.0.0.0/8`. No second
//! node, no DNS seeds, no outbound dial.

use bitcoin::p2p::address::Address;
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message_network::VersionMessage;
use bitcoin::p2p::ServiceFlags;
use rbitcoin_net::V2PlainSession;
use rbitcoin_node::{run_p2p, NodeConfig};
use rbitcoin_primitives::Network;
use rbitcoin_test::TestDatadir;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn llvm_cov_wall(default_secs: u64, llvm_secs: u64) -> Duration {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        Duration::from_secs(llvm_secs)
    } else {
        Duration::from_secs(default_secs)
    }
}

type StartupLogs = Vec<(rbitcoin_log::Level, String)>;

fn spawn_run_p2p_logs(
    cfg: NodeConfig,
) -> tokio::task::JoinHandle<(Result<(), rbitcoin_node::NodeError>, StartupLogs)> {
    match std::fs::remove_dir_all(cfg.datadir.path().join("run")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("clear published listeners: {e}"),
    }
    tokio::task::spawn_blocking(move || {
        let _block = rbitcoin_net::BlockingRegion::enter();
        rbitcoin_log::capture_logs(true);
        let result = tokio::runtime::Handle::current().block_on(run_p2p(cfg));
        let logs = rbitcoin_log::take_logs();
        rbitcoin_log::capture_logs(false);
        (result, logs)
    })
}

async fn stop_logged(
    rpc: SocketAddr,
    node: tokio::task::JoinHandle<(Result<(), rbitcoin_node::NodeError>, StartupLogs)>,
) -> StartupLogs {
    let _ = jsonrpc(rpc, "stop", json!([])).await;
    match tokio::time::timeout(Duration::from_secs(20), node).await {
        Ok(Ok((Ok(()), logs))) => logs,
        Ok(Ok((Err(e), _))) => panic!("run_p2p error after stop: {e}"),
        Ok(Err(e)) => panic!("run_p2p join: {e}"),
        Err(_) => panic!("run_p2p did not exit after stop"),
    }
}

async fn published_listener(datadir: &Path, name: &str) -> SocketAddr {
    let path = datadir.join("run").join(format!("{name}.addr"));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(addr) = text.trim().parse::<SocketAddr>() {
                if addr.port() != 0 {
                    return addr;
                }
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "bound {name} address was not published at {}",
                path.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn jsonrpc(addr: SocketAddr, method: &str, params: Value) -> Value {
    rbitcoin_test::jsonrpc::jsonrpc(addr, method, params).await
}

fn base_cfg(dir: &Path) -> NodeConfig {
    let mut cfg = NodeConfig::default()
        .with_datadir(dir)
        .with_network(Network::Regtest)
        .with_tiny_heads();
    cfg.listen.use_seeds = false;
    cfg
}

fn write_token(dir: &Path) {
    std::fs::write(dir.join("rpc.token"), "pass\n").unwrap();
}

fn log_text(logs: &StartupLogs) -> String {
    logs.iter()
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn handshake(bind_ip: Ipv4Addr, to: SocketAddr, agent: &str) -> V2PlainSession {
    let sock = tokio::net::TcpSocket::new_v4().expect("v4 socket");
    sock.bind(SocketAddr::new(IpAddr::V4(bind_ip), 0))
        .unwrap_or_else(|e| panic!("bind {bind_ip}: {e}"));
    let stream = tokio::time::timeout(Duration::from_secs(5), sock.connect(to))
        .await
        .unwrap_or_else(|_| panic!("connect {bind_ip} to {to} timed out"))
        .unwrap_or_else(|e| panic!("connect {bind_ip} to {to}: {e}"));
    let mut raw = V2PlainSession::outbound_bip324(stream)
        .await
        .expect("bip324");
    let ver = VersionMessage {
        version: 70016,
        services: ServiceFlags::NETWORK | ServiceFlags::WITNESS,
        timestamp: 0,
        receiver: Address::new(&to, ServiceFlags::NONE),
        sender: Address::new(&to, ServiceFlags::NONE),
        nonce: u64::from(bind_ip.octets()[3]),
        user_agent: agent.into(),
        start_height: 0,
        relay: true,
    };
    for msg in [NetworkMessage::Version(ver), NetworkMessage::Verack] {
        raw.write_contents(&rbitcoin_net::encode_v2_contents(msg).expect("encode"))
            .await
            .expect("write handshake");
    }
    raw
}

fn mapped_as(rows: &Value, ip: Ipv4Addr) -> u64 {
    let prefix = format!("{ip}:");
    let row = rows
        .as_array()
        .unwrap_or_else(|| panic!("getpeerinfo is not a list: {rows}"))
        .iter()
        .find(|row| {
            row["addr"]
                .as_str()
                .is_some_and(|addr| addr.starts_with(&prefix))
        })
        .unwrap_or_else(|| panic!("no peer {ip} in {rows}"));
    row["mapped_as"]
        .as_u64()
        .unwrap_or_else(|| panic!("peer {ip} has no mapped_as: {row}"))
}

async fn wait_peers(rpc: SocketAddr, n: usize) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let info = jsonrpc(rpc, "getpeerinfo", json!([])).await;
        let count = info["result"].as_array().map(|a| a.len()).unwrap_or(0);
        if count >= n {
            return info["result"].clone();
        }
        if Instant::now() >= deadline {
            panic!("expected {n} peers, got {info}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn assert_opened(logs: &StartupLogs, len: usize, digest: &str) {
    let text = log_text(logs);
    assert!(
        text.contains(&format!("({len} bytes, digest {digest})")),
        "opened line missing from {text}"
    );
    assert!(
        !text.contains("Sanity check of asmap file"),
        "a valid asmap was refused: {text}"
    );
}

/// Loopback peers report the ASN from `{datadir}/ip_asn.dat`, then a relative
/// `--asmap` of a lone return maps the same peer to ASN 1.
#[tokio::test(flavor = "multi_thread")]
async fn asmap_loopback_peers_report_their_asn() {
    let wall = llvm_cov_wall(20, 60);
    let fut = async {
        let td = TestDatadir::new().unwrap();
        let dir = td.path();
        std::fs::write(dir.join("ip_asn.dat"), rbitcoin_net::LOOPBACK_ASMAP).unwrap();
        write_token(&dir);
        let mut cfg = base_cfg(&dir).with_p2p_listen("127.0.0.1:0".parse().unwrap());
        cfg.rpc.listen = Some("127.0.0.1:0".parse().unwrap());
        let node = spawn_run_p2p_logs(cfg);
        let rpc = published_listener(&dir, "rpc").await;
        let p2p = published_listener(&dir, "p2p").await;
        let _a = handshake(Ipv4Addr::new(127, 0, 0, 1), p2p, "/asmap-1/").await;
        let _b = handshake(Ipv4Addr::new(127, 0, 0, 2), p2p, "/asmap-2/").await;
        let _c = handshake(Ipv4Addr::new(127, 0, 0, 3), p2p, "/asmap-3/").await;
        let rows = wait_peers(rpc, 3).await;
        assert_eq!(mapped_as(&rows, Ipv4Addr::new(127, 0, 0, 1)), 1, "{rows}");
        assert_eq!(mapped_as(&rows, Ipv4Addr::new(127, 0, 0, 2)), 2, "{rows}");
        assert_eq!(mapped_as(&rows, Ipv4Addr::new(127, 0, 0, 3)), 9, "{rows}");
        let logs = stop_logged(rpc, node).await;
        assert_opened(&logs, rbitcoin_net::LOOPBACK_ASMAP.len(), "0d2045c8");

        let td = TestDatadir::new().unwrap();
        let dir = td.path();
        std::fs::write(dir.join("asn1.dat"), rbitcoin_net::LONE_RETURN_ASMAP).unwrap();
        write_token(&dir);
        let mut cfg = base_cfg(&dir).with_p2p_listen("127.0.0.1:0".parse().unwrap());
        cfg.asmap = Some(PathBuf::from("asn1.dat"));
        cfg.rpc.listen = Some("127.0.0.1:0".parse().unwrap());
        let node = spawn_run_p2p_logs(cfg);
        let rpc = published_listener(&dir, "rpc").await;
        let p2p = published_listener(&dir, "p2p").await;
        let _peer = handshake(Ipv4Addr::new(127, 0, 0, 1), p2p, "/asmap-lone/").await;
        let rows = wait_peers(rpc, 1).await;
        assert_eq!(mapped_as(&rows, Ipv4Addr::new(127, 0, 0, 1)), 1, "{rows}");
        let logs = stop_logged(rpc, node).await;
        assert_opened(&logs, 3, "709e80c8");
    };
    tokio::time::timeout(wall, fut).await.unwrap_or_else(|_| {
        panic!("asmap_loopback_peers_report_their_asn wall timeout ({wall:?})")
    });
}

fn assert_refused(logs: &StartupLogs, phrase: &str) {
    let text = log_text(logs);
    assert!(text.contains(phrase), "missing {phrase:?} in {text}");
    assert!(
        !text.contains("Opened asmap file"),
        "a refused asmap loaded: {text}"
    );
}

async fn boot_refused(cfg: NodeConfig, phrase: &str) {
    let node = spawn_run_p2p_logs(cfg);
    match tokio::time::timeout(Duration::from_secs(20), node).await {
        Ok(Ok((Ok(()), logs))) => assert_refused(&logs, phrase),
        Ok(Ok((Err(e), logs))) => panic!("run_p2p error: {e}; logs={}", log_text(&logs)),
        Ok(Err(e)) => panic!("run_p2p join: {e}"),
        Err(_) => panic!("refused asmap boot did not exit"),
    }
}

/// Each bad asmap is refused at startup. The node exits once tip entry is
/// ready (`max_run_secs` 0) and never dials a peer.
#[tokio::test(flavor = "multi_thread")]
async fn asmap_refused_file_uses_prefix_netgroups() {
    let wall = llvm_cov_wall(20, 60);
    let cases: &[(&str, &[u8], &str)] = &[
        (
            "second-default.dat",
            rbitcoin_net::ASMAP_SECOND_DEFAULT,
            "Sanity check of asmap file",
        ),
        (
            "trailing-one.dat",
            rbitcoin_net::ASMAP_TRAILING_ONE,
            "Sanity check of asmap file",
        ),
        (
            "eight-zeros.dat",
            rbitcoin_net::ASMAP_EIGHT_TRAILING_ZEROS,
            "Sanity check of asmap file",
        ),
        (
            "two-short.dat",
            rbitcoin_net::ASMAP_TWO_SHORT_MATCHES,
            "Sanity check of asmap file",
        ),
        (
            "match-past.dat",
            rbitcoin_net::ASMAP_MATCH_PAST_END,
            "Sanity check of asmap file",
        ),
        (
            "jump-past.dat",
            rbitcoin_net::ASMAP_JUMP_PAST_FILE,
            "Sanity check of asmap file",
        ),
        (
            "jump-short.dat",
            rbitcoin_net::ASMAP_JUMP_SHORT_TAIL,
            "Sanity check of asmap file",
        ),
        (
            "jump-off.dat",
            rbitcoin_net::ASMAP_JUMP_OFF_END,
            "Sanity check of asmap file",
        ),
    ];
    let fut = async {
        for (name, bytes, phrase) in cases {
            let td = TestDatadir::new().unwrap();
            let dir = td.path();
            std::fs::write(dir.join(name), bytes).unwrap();
            let mut cfg = base_cfg(&dir).with_p2p_listen("127.0.0.1:0".parse().unwrap());
            cfg.asmap = Some(PathBuf::from(name));
            cfg.max_run_secs = Some(0);
            boot_refused(cfg, phrase).await;
        }

        let td = TestDatadir::new().unwrap();
        let dir = td.path();
        std::fs::write(dir.join("empty.dat"), []).unwrap();
        let mut cfg = base_cfg(&dir).with_p2p_listen("127.0.0.1:0".parse().unwrap());
        cfg.asmap = Some(PathBuf::from("empty.dat"));
        cfg.max_run_secs = Some(0);
        boot_refused(cfg, "Sanity check of asmap file").await;

        let td = TestDatadir::new().unwrap();
        let dir = td.path();
        std::fs::write(dir.join("one.dat"), [0u8]).unwrap();
        let mut cfg = base_cfg(&dir).with_p2p_listen("127.0.0.1:0".parse().unwrap());
        cfg.asmap = Some(PathBuf::from("one.dat"));
        cfg.max_run_secs = Some(0);
        boot_refused(cfg, "Sanity check of asmap file").await;

        let td = TestDatadir::new().unwrap();
        let dir = td.path();
        let mut cfg = base_cfg(&dir).with_p2p_listen("127.0.0.1:0".parse().unwrap());
        cfg.asmap = Some(PathBuf::from("no-such-asmap"));
        cfg.max_run_secs = Some(0);
        boot_refused(cfg, "Failed to open asmap file").await;
    };
    tokio::time::timeout(wall, fut).await.unwrap_or_else(|_| {
        panic!("asmap_refused_file_uses_prefix_netgroups wall timeout ({wall:?})")
    });
}
