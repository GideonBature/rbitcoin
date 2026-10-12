//! Short regtest IBD smoke: seed node with a few blocks, peer catch-up via
//! [`P2PNode::sync`]. Hits the main IBD loop, archive pipeline, confirm engine,
//! peer_io, and dial/events paths without live datadirs.

use bitcoin::{Block, BlockHash};
use rbitcoin_consensus::{ChainParams, Milestone};
use rbitcoin_net::{IbdConfig, P2PNode};
use rbitcoin_query::Query;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn tmp_dir(label: &str) -> rbitcoin_store::testutil::TempDir {
    rbitcoin_store::testutil::TempDir::labeled(label).unwrap()
}

fn mine(prev: BlockHash, time: u32, height: u32) -> Block {
    rbitcoin_consensus::mine_empty_regtest(prev, time, height)
}

async fn start_node(dir: &std::path::Path) -> P2PNode {
    let q = Query::open_or_create_tiny(dir.join("store")).unwrap();
    P2PNode::start(
        "127.0.0.1:0".parse().unwrap(),
        q,
        ChainParams::regtest(),
        Milestone::NONE,
    )
    .await
    .expect("listen")
}

fn seed_chain(node: &P2PNode, blocks: u32) {
    let genesis = bitcoin::blockdata::constants::genesis_block(bitcoin::Network::Regtest);
    node.ingest_block(0, genesis.clone()).unwrap();
    let mut tip = genesis.block_hash();
    let mut time = genesis.header.time;
    for h in 1..=blocks {
        let b = mine(tip, time + 600, h);
        tip = b.block_hash();
        time = b.header.time;
        node.ingest_block(h, b).unwrap();
    }
}

/// Cancel flag exits IBD cooperatively without hanging.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ibd_cancellable_exits_when_flag_set() {
    let seed_dir = tmp_dir("seed-cancel");
    let peer_dir = tmp_dir("peer-cancel");

    let seed = start_node(&seed_dir).await;
    seed_chain(&seed, 4);

    let peer = start_node(&peer_dir).await;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_c = cancel.clone();
    // Flip cancel after a short delay so dial/handshake may start.
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_c.store(true, Ordering::SeqCst);
    });
    let mut cfg = IbdConfig::for_test();
    cfg.target_peers = 1;
    // Cancelled IBD should return Ok (partial) or complete if race finishes first.
    let _ = peer
        .sync_cancellable(
            &[rbitcoin_net::NetAddr::Ip(seed.local_addr)],
            cfg,
            Some(cancel),
        )
        .await;

    seed.shutdown().await;
    peer.shutdown().await;
    let _ = std::fs::remove_dir_all(&seed_dir);
    let _ = std::fs::remove_dir_all(&peer_dir);
}

/// Empty peer list is a clean protocol error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ibd_no_peers_errors() {
    let dir = tmp_dir("empty");
    let node = start_node(&dir).await;
    let err = node.sync(&[], IbdConfig::for_test()).await.unwrap_err();
    assert!(err.to_string().contains("no peers") || err.to_string().contains("protocol"));
    node.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Unreachable peer → dial fail / no peers connected (main-loop error arm).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ibd_unreachable_peer_errors() {
    let dir = tmp_dir("unreachable");
    let node = start_node(&dir).await;
    let mut cfg = IbdConfig::for_test();
    cfg.target_peers = 1;
    cfg.connect_timeout = Duration::from_millis(150);
    // TEST-NET-1 documentation address — closed / non-listening.
    let dead: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
    let err = node
        .sync(&[rbitcoin_net::NetAddr::Ip(dead)], cfg)
        .await
        .unwrap_err();
    let s = err.to_string();
    assert!(
        s.contains("no peers") || s.contains("protocol") || s.contains("connect"),
        "unexpected: {s}"
    );
    node.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}
