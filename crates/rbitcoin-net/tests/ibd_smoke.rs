//! Short regtest IBD smoke: seed node with a few blocks, peer catch-up via
//! [`P2PNode::sync`]. Hits the main IBD loop, archive pipeline, confirm engine,
//! peer_io, and dial/events paths without live datadirs.

use bitcoin::hashes::Hash;
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

fn logs_from_sync() -> impl Fn() -> Vec<String> {
    rbitcoin_log::capture_logs(true);
    || {
        rbitcoin_log::take_logs()
            .into_iter()
            .map(|(_, line)| line)
            .collect()
    }
}

/// One live seed and a higher target reaches the redial clock. An empty body
/// queue must not claim it noted rows, and a store with no Class A bodies must
/// not announce a seed count of zero.
#[tokio::test(flavor = "current_thread")]
async fn ibd_redials_immediately_and_does_not_invent_a_rehydrate() {
    let seed_dir = tmp_dir("seed-redial");
    let peer_dir = tmp_dir("peer-redial");
    let seed = start_node(&seed_dir).await;
    seed_chain(&seed, 2);
    let peer = start_node(&peer_dir).await;
    peer.hub.ensure_genesis().unwrap();
    let archived = peer.hub.query.archived_block_count().unwrap();
    let mut cfg = IbdConfig::for_test();
    cfg.target_peers = 2;
    let take = logs_from_sync();
    // Cancel during the initial dial returns before rehydrate and before the
    // redial line. This chain is two blocks, so catch-up exits on its own.
    let finished = tokio::time::timeout(
        Duration::from_secs(20),
        peer.sync_cancellable(&[rbitcoin_net::NetAddr::Ip(seed.local_addr)], cfg, None),
    )
    .await;
    let logs = take();
    rbitcoin_log::capture_logs(false);
    assert!(finished.is_ok(), "short-chain ibd did not finish: {logs:?}");
    assert!(
        logs.iter().any(|line| line.contains("ibd: redialing")),
        "a short peer set redials on the first loop: {logs:?}"
    );
    assert!(
        logs.iter().all(|line| !line.contains("noted 0")),
        "an empty body queue is not reported as noted: {logs:?}"
    );
    let seed_line = format!("store has {archived} Class A bodies (seed)");
    if archived > 0 {
        assert!(
            logs.iter().any(|line| line.contains(&seed_line)),
            "Class A bodies already in the store are named: {logs:?}"
        );
    } else {
        assert!(
            logs.iter()
                .all(|line| !line.contains("Class A bodies (seed)")),
            "an empty store does not announce a Class A seed: {logs:?}"
        );
    }
    seed.shutdown().await;
    peer.shutdown().await;
    let _ = std::fs::remove_dir_all(&seed_dir);
    let _ = std::fs::remove_dir_all(&peer_dir);
}

/// A block left in the RAM body queue above the tip is noted into confirm.
#[tokio::test(flavor = "current_thread")]
async fn ibd_notes_a_body_queue_row_above_the_tip() {
    let seed_dir = tmp_dir("seed-bq");
    let peer_dir = tmp_dir("peer-bq");
    let seed = start_node(&seed_dir).await;
    seed_chain(&seed, 1);
    let peer = start_node(&peer_dir).await;
    peer.hub.ensure_genesis().unwrap();
    let gen = peer.hub.tip_hash().unwrap();
    let genesis = bitcoin::blockdata::constants::genesis_block(bitcoin::Network::Regtest);
    let block = mine(gen, genesis.header.time + 600, 1);
    let fk = peer.hub.ensure_header_fk(&block.header).unwrap();
    let raw = bitcoin::consensus::encode::serialize(&block);
    peer.hub
        .query
        .block_queue_enqueue(1, block.block_hash().to_byte_array(), fk.0, &raw)
        .unwrap();
    let mut cfg = IbdConfig::for_test();
    cfg.target_peers = 1;
    let take = logs_from_sync();
    // Same dial-cancel hole as the redial journey: the noted line is written
    // only after the handshake returns.
    let finished = tokio::time::timeout(
        Duration::from_secs(20),
        peer.sync_cancellable(&[rbitcoin_net::NetAddr::Ip(seed.local_addr)], cfg, None),
    )
    .await;
    let logs = take();
    rbitcoin_log::capture_logs(false);
    assert!(finished.is_ok(), "short-chain ibd did not finish: {logs:?}");
    assert!(
        logs.iter()
            .any(|line| line.contains("ibd: rehydrate: noted 1 in-RAM body queue entries")),
        "the queued block is noted: {logs:?}"
    );
    seed.shutdown().await;
    peer.shutdown().await;
    let _ = std::fs::remove_dir_all(&seed_dir);
    let _ = std::fs::remove_dir_all(&peer_dir);
}
