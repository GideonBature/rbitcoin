//! Listen / dial / sync / tip-follow orchestration.

use crate::cache::BlockCache;
use crate::chain::{AcceptOutcome, ChainHub};
use crate::error::NetError;
use crate::ibd::IbdConfig;
use crate::peer::{
    connect_and_handshake_timed, inbound_connect_and_handshake, peer_session_with,
    FollowSessionMeta, HandshakePolicy, HANDSHAKE_TIMEOUT,
};
use crate::peer_dos::{inbound_semaphore, DEFAULT_MAX_INBOUND};
use crate::peers::{DialRequest, DialTarget, LivePeer, PeerConnType, PeerHub};
use crate::v2::{V2Reader, V2Writer};
use bitcoin::p2p::Magic;
use bitcoin::Block;
use bitcoin::BlockHash;
use rbitcoin_consensus::{signet_magic, ChainParams, Milestone};
use rbitcoin_query::Query;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Running P2P node handle (listen + optional outbound sync / tip follow).
pub struct P2PNode {
    /// Shared RAM cache (also on hub).
    pub cache: Arc<BlockCache>,
    /// Shared query store (also on hub).
    pub query: Arc<Query>,
    pub hub: Arc<ChainHub>,
    pub local_addr: SocketAddr,
    magic: Magic,
    shutdown: Arc<AtomicBool>,
    /// Live outbound tip-follow sessions (inc/dec inside session task).
    follow_live: Arc<AtomicUsize>,
    tasks: Vec<JoinHandle<()>>,
    /// Nested inbound / dial session tasks (not in `tasks` so accept/dial
    /// abort does not leave them running through process exit).
    session_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// Live sessions for RPC getpeerinfo / addnode / disconnectnode.
    pub peers: Arc<PeerHub>,
    user_agent: String,
    /// Inbound session cap used by the accept loop (not process env).
    pub max_inbound: usize,
    /// Shared inbound slots across all listen sockets.
    inbound_sem: Arc<tokio::sync::Semaphore>,
    dialer: crate::socks::Dialer,
    /// Wakes accept, dial, and connect-retry so `shutdown` does not sit out
    /// its grace waiting on a recv or a 2s tick.
    stop_tx: tokio::sync::watch::Sender<bool>,
}

impl P2PNode {
    /// Bind listener. Serves getheaders/getdata and participates in tip announce/follow.
    pub async fn start(
        listen: SocketAddr,
        query: Query,
        params: ChainParams,
        milestone: Milestone,
    ) -> Result<Self, NetError> {
        Self::start_with_agent(
            listen,
            query,
            params,
            milestone,
            default_user_agent(),
            DEFAULT_MAX_INBOUND,
        )
        .await
    }

    /// Like [`Self::start`] with an explicit BIP14 user-agent (RPC `subversion`)
    /// and inbound session cap.
    pub async fn start_with_agent(
        listen: SocketAddr,
        query: Query,
        params: ChainParams,
        milestone: Milestone,
        user_agent: String,
        max_inbound: usize,
    ) -> Result<Self, NetError> {
        Self::start_with_dialer(
            listen,
            query,
            params,
            milestone,
            user_agent,
            max_inbound,
            crate::socks::Dialer::Direct,
        )
        .await
    }

    /// Like [`Self::start_with_agent`] with an outbound [`crate::Dialer`].
    pub async fn start_with_dialer(
        listen: SocketAddr,
        query: Query,
        params: ChainParams,
        milestone: Milestone,
        user_agent: String,
        max_inbound: usize,
        dialer: crate::socks::Dialer,
    ) -> Result<Self, NetError> {
        let bind = (max_inbound > 0).then_some(listen);
        Self::start_inner(
            bind,
            query,
            params,
            milestone,
            user_agent,
            max_inbound,
            dialer,
        )
        .await
    }

    /// Outbound dials only: no P2P `TcpListener`. `local_addr` is `127.0.0.1:0`.
    pub async fn start_outbound_only(
        query: Query,
        params: ChainParams,
        milestone: Milestone,
        user_agent: String,
        max_inbound: usize,
        dialer: crate::socks::Dialer,
    ) -> Result<Self, NetError> {
        Self::start_inner(
            None,
            query,
            params,
            milestone,
            user_agent,
            max_inbound,
            dialer,
        )
        .await
    }

    async fn start_inner(
        listen: Option<SocketAddr>,
        query: Query,
        params: ChainParams,
        milestone: Milestone,
        user_agent: String,
        max_inbound: usize,
        dialer: crate::socks::Dialer,
    ) -> Result<Self, NetError> {
        let magic = magic_for_params(&params);
        let hub = ChainHub::into_arc(ChainHub::new(query, params, milestone));
        hub.ensure_genesis()?;
        let cache = hub.cache.clone();
        let query = hub.query.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let (stop_tx, _stop_rx) = tokio::sync::watch::channel(false);

        let (listener, local_addr) = if let Some(addr) = listen {
            let listener = TcpListener::bind(addr).await?;
            let local_addr = listener.local_addr()?;
            (Some(listener), local_addr)
        } else {
            (None, SocketAddr::from(([127, 0, 0, 1], 0)))
        };

        let peers = PeerHub::new();
        let (dial_tx, mut dial_rx) = tokio::sync::mpsc::unbounded_channel::<DialRequest>();
        peers.set_dialer(dial_tx);

        let max_inbound = if listener.is_some() {
            max_inbound.max(1)
        } else {
            0
        };
        let inbound_sem = inbound_semaphore(max_inbound.max(1));
        let session_tasks = Arc::new(Mutex::new(Vec::<JoinHandle<()>>::new()));
        let mut tasks = Vec::new();
        if let Some(listener) = listener {
            tasks.push(spawn_inbound_accept(
                listener,
                local_addr,
                hub.clone(),
                peers.clone(),
                user_agent.clone(),
                magic,
                max_inbound.max(1),
                inbound_sem.clone(),
                shutdown.clone(),
                session_tasks.clone(),
                stop_tx.subscribe(),
            ));
        }

        let follow_live = Arc::new(AtomicUsize::new(0));
        let dial_hub = hub.clone();
        let dial_peers = peers.clone();
        let dial_ua = user_agent.clone();
        let dial_live = follow_live.clone();
        let dial_shutdown = shutdown.clone();
        let sessions_dial = session_tasks.clone();
        let dialer_task = dialer.clone();
        let mut dial_stop = stop_tx.subscribe();
        let dial_task = tokio::spawn(async move {
            loop {
                let req = tokio::select! {
                    biased;
                    _ = dial_stop.changed() => break,
                    r = dial_rx.recv() => match r {
                        Some(req) => req,
                        None => break,
                    },
                };
                if dial_shutdown.load(Ordering::SeqCst) {
                    break;
                }
                let hub = dial_hub.clone();
                let peers = dial_peers.clone();
                let ua = dial_ua.clone();
                let live = dial_live.clone();
                let d = dialer_task.clone();
                let (ah_tx, ah_rx) = tokio::sync::oneshot::channel::<tokio::task::AbortHandle>();
                let h = tokio::spawn(async move {
                    let _ = run_outbound_session_with_abort(
                        req.target,
                        req.in_flight,
                        magic,
                        local_addr,
                        hub,
                        peers,
                        ua,
                        live,
                        req.typ,
                        ah_rx,
                        d,
                    )
                    .await;
                });
                let _ = ah_tx.send(h.abort_handle());
                push_session_task(&sessions_dial, h);
            }
        });
        tasks.push(dial_task);

        // DNS for remembered `--connect` / `addnode add` runs on the blocking
        // pool (`PeerHub::redial_remembered_off_runtime`).
        let retry_peers = peers.clone();
        let retry_shutdown = shutdown.clone();
        let mut retry_stop = stop_tx.subscribe();
        let retry_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(REDIAL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                tokio::select! {
                    biased;
                    _ = retry_stop.changed() => break,
                    _ = interval.tick() => {}
                }
                if retry_shutdown.load(Ordering::SeqCst) {
                    break;
                }
                retry_peers.redial_remembered_off_runtime().await;
            }
        });
        tasks.push(retry_task);

        Ok(Self {
            cache,
            query,
            hub,
            local_addr,
            magic,
            shutdown,
            follow_live,
            tasks,
            session_tasks,
            peers,
            user_agent,
            max_inbound,
            inbound_sem,
            dialer,
            stop_tx,
        })
    }

    /// Outbound TCP path (direct or SOCKS).
    pub fn dialer(&self) -> crate::socks::Dialer {
        self.dialer.clone()
    }

    pub fn magic(&self) -> Magic {
        self.magic
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Bind an additional listen socket (Core multi-`-bind`).
    pub async fn add_listen(&mut self, listen: SocketAddr) -> Result<SocketAddr, NetError> {
        let listener = TcpListener::bind(listen).await?;
        let local_addr = listener.local_addr()?;
        let accept_task = spawn_inbound_accept(
            listener,
            local_addr,
            self.hub.clone(),
            self.peers.clone(),
            self.user_agent.clone(),
            self.magic,
            self.max_inbound,
            self.inbound_sem.clone(),
            self.shutdown.clone(),
            self.session_tasks.clone(),
            self.stop_tx.subscribe(),
        );
        self.tasks.push(accept_task);
        Ok(local_addr)
    }

    /// Number of live outbound tip-follow sessions.
    pub fn follow_live_count(&self) -> usize {
        self.follow_live.load(Ordering::SeqCst)
    }

    pub fn tip_height(&self) -> Option<u32> {
        self.hub.tip_height()
    }

    /// Push a validated block into cache + store.
    pub fn ingest_block(&self, height: u32, block: Block) -> Result<(), NetError> {
        let _ = height;
        match self.hub.accept_received_block(block)? {
            AcceptOutcome::Accepted { .. } | AcceptOutcome::AlreadyHave => Ok(()),
            AcceptOutcome::IgnoredWeaker => Err(NetError::Protocol("weaker tip ignored")),
        }
    }

    /// IBD / catch-up: multi-peer densify across `peers`.
    ///
    /// This is the only history-sync path. Tip-follow is [`Self::follow_from`].
    pub async fn sync(&self, peers: &[crate::NetAddr], cfg: IbdConfig) -> Result<u32, NetError> {
        self.sync_cancellable(peers, cfg, None).await
    }

    /// IBD with optional cooperative cancel flag (SIGINT / SIGTERM path).
    pub async fn sync_cancellable(
        &self,
        peers: &[crate::NetAddr],
        cfg: IbdConfig,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<u32, NetError> {
        crate::ibd::ibd_cancellable(
            self.hub.clone(),
            self.magic,
            self.local_addr,
            peers,
            cfg,
            cancel,
            Some(Arc::clone(&self.peers)),
        )
        .await
    }

    /// IBD with default window (1024 concurrent getdata, 16/peer).
    pub async fn sync_default(&self, peers: &[crate::NetAddr]) -> Result<u32, NetError> {
        self.sync(peers, IbdConfig::default()).await
    }

    /// Persistent outbound peer: tip-follow + announce for the session lifetime.
    ///
    /// Handshake completes on this task (so an 8s connect timeout is meaningful);
    /// the session itself is spawned so the caller does not sit in the read loop.
    /// After handshake the session sends `getheaders` from our tip locator so
    /// any gap (e.g. blocks mined during SH materialize) is filled actively.
    /// Call [`Self::sync`] first when far behind (multi-thousand height IBD).
    pub async fn follow_from(&mut self, peer: SocketAddr) -> Result<(), NetError> {
        self.follow_from_net(crate::NetAddr::Ip(peer)).await
    }

    pub async fn follow_from_net(&mut self, peer: crate::NetAddr) -> Result<(), NetError> {
        self.follow_from_net_as(peer, PeerConnType::OutboundFullRelay)
            .await
    }

    /// [`Self::follow_from_net`] with the session's `connection_type`
    /// (`Manual` for a `--connect` target).
    pub async fn follow_from_net_as(
        &mut self,
        peer: crate::NetAddr,
        typ: PeerConnType,
    ) -> Result<(), NetError> {
        let target = DialTarget::from_net(peer);
        let prepared = prepare_outbound_session(
            target,
            None,
            self.magic,
            self.local_addr,
            self.hub.clone(),
            self.peers.clone(),
            self.user_agent.clone(),
            self.follow_live.clone(),
            typ,
            self.dialer.clone(),
        )
        .await?;
        let handle = tokio::spawn(async move {
            let _ = run_prepared_outbound(prepared).await;
        });
        self.tasks.push(handle);
        Ok(())
    }

    pub async fn wait_height(&self, height: u32, timeout: Duration) -> Result<(), NetError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.tip_height().unwrap_or(0) >= height {
                let hub = Arc::clone(&self.hub);
                tokio::task::spawn_blocking(move || hub.wait_tip_stable_for_rpc())
                    .await
                    .expect("wait tip idle");
                if self.tip_height().unwrap_or(0) >= height {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(NetError::Timeout);
            }
            tokio::select! {
                _ = self.hub.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }

    pub async fn wait_tip_hash(&self, hash: BlockHash, timeout: Duration) -> Result<(), NetError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.hub.tip_hash() == Some(hash) {
                let hub = Arc::clone(&self.hub);
                tokio::task::spawn_blocking(move || hub.wait_tip_stable_for_rpc())
                    .await
                    .expect("wait tip idle");
                if self.hub.tip_hash() == Some(hash) {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(NetError::Timeout);
            }
            tokio::select! {
                _ = self.hub.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }

    pub async fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.stop_tx.send(true);
        for p in self.peers.live_peers() {
            p.request_disconnect();
        }
        let mut join = Vec::new();
        join.append(&mut self.tasks);
        if let Ok(mut g) = self.session_tasks.lock() {
            join.append(&mut *g);
        }
        let grace = tokio::time::Instant::now() + Duration::from_millis(750);
        while tokio::time::Instant::now() < grace && join.iter().any(|t| !t.is_finished()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        for t in &join {
            t.abort();
        }
        let _ = tokio::time::timeout(Duration::from_millis(750), async {
            for t in join {
                let _ = t.await;
            }
        })
        .await;
    }
}

impl Drop for P2PNode {
    fn drop(&mut self) {
        // The connect-retry interval runs until this flag or an abort.
        // Dropping the handle without `shutdown` must not pin the test runtime.
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.stop_tx.send(true);
        for t in &self.tasks {
            t.abort();
        }
        if let Ok(g) = self.session_tasks.lock() {
            for t in g.iter() {
                t.abort();
            }
        }
    }
}

fn push_session_task(bag: &Mutex<Vec<JoinHandle<()>>>, h: JoinHandle<()>) {
    if let Ok(mut g) = bag.lock() {
        g.retain(|t| !t.is_finished());
        g.push(h);
    }
}

#[allow(clippy::too_many_arguments)] // call-site args stay unbundled
fn spawn_inbound_accept(
    listener: TcpListener,
    local_addr: SocketAddr,
    hub: Arc<ChainHub>,
    peers: Arc<PeerHub>,
    user_agent: String,
    magic: Magic,
    max_inbound: usize,
    inbound_sem: Arc<tokio::sync::Semaphore>,
    shutdown: Arc<AtomicBool>,
    session_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if shutdown.load(Ordering::SeqCst) {
                break;
            }
            let accept = tokio::select! {
                biased;
                _ = stop.changed() => break,
                a = listener.accept() => a,
            };
            match accept {
                Ok((stream, peer_addr)) => {
                    if peers.inbound_discouraged(peer_addr) {
                        rbitcoin_log::info!("p2p: reject discouraged inbound {peer_addr}");
                        drop(stream);
                        continue;
                    }
                    let permit = match inbound_sem.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => {
                            if !peers.try_evict_inbound() {
                                rbitcoin_log::warn!(
                                    "p2p: reject inbound {peer_addr} (at max_inbound={max_inbound})"
                                );
                                drop(stream);
                                continue;
                            }
                            match tokio::time::timeout(
                                Duration::from_millis(500),
                                inbound_sem.clone().acquire_owned(),
                            )
                            .await
                            {
                                Ok(Ok(p)) => p,
                                _ => {
                                    rbitcoin_log::warn!(
                                        "p2p: reject inbound {peer_addr} (evict slot wait)"
                                    );
                                    drop(stream);
                                    continue;
                                }
                            }
                        }
                    };
                    let our = local_addr;
                    let hub = hub.clone();
                    let height = hub.tip_height().map(|h| h as i32).unwrap_or(0);
                    let tip_rx = hub.subscribe_tips();
                    let peers = peers.clone();
                    let ua = user_agent.clone();
                    let sessions = session_tasks.clone();
                    let (ah_tx, ah_rx) =
                        tokio::sync::oneshot::channel::<tokio::task::AbortHandle>();
                    let h = tokio::spawn(async move {
                        let _session_slot = permit;
                        let (_ver, reader, writer, wire, tcp_shutdown, sess) =
                            match inbound_connect_and_handshake(
                                stream, magic, our, peer_addr, height, &ua, &peers, our,
                            )
                            .await
                            {
                                Ok(x) => x,
                                Err(e) => {
                                    rbitcoin_log::debug!(
                                        "p2p: inbound handshake {peer_addr} failed: {e}"
                                    );
                                    return;
                                }
                            };
                        sess.attach_tcp_shutdown(tcp_shutdown);
                        if let Ok(ah) = ah_rx.await {
                            sess.set_session_abort(ah);
                        }
                        if let Some(mp) = hub.mempool() {
                            sess.set_inv_gen_floor(mp.next_accept_gen());
                        }
                        sess.attach_wire(wire);
                        let id = sess.id;
                        let meta = FollowSessionMeta {
                            peer: Some(peer_addr),
                            live: None,
                            session: Some(sess),
                        };
                        let _ = peer_session_with(reader, writer, magic, hub, tip_rx, meta).await;
                        peers.unregister(id);
                    });
                    let _ = ah_tx.send(h.abort_handle());
                    push_session_task(&sessions, h);
                }
                Err(e) => {
                    if accept_error_ends_listen(&e) {
                        rbitcoin_log::warn!("p2p: accept ended: {e}");
                        break;
                    }
                    rbitcoin_log::warn!("p2p: accept retry: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    })
}

fn default_user_agent() -> String {
    rbitcoin_primitives::rbitcoin_subversion(env!("CARGO_PKG_VERSION"), &[] as &[&str])
        .unwrap_or_else(|_| format!("/rbitcoin:{}/", env!("CARGO_PKG_VERSION")))
}

struct PreparedOutbound {
    peer_hint: SocketAddr,
    magic: Magic,
    hub: Arc<ChainHub>,
    peers: Arc<PeerHub>,
    follow_live: Arc<AtomicUsize>,
    reader: V2Reader,
    writer: V2Writer,
    sess: Arc<LivePeer>,
    id: u64,
}

/// True when this crate was compiled with the `test-hooks` feature.
///
/// Journeys turn that on. A workspace build unifies it into the node binary,
/// which then uses the short redial and SAM retry. The node warns at startup.
pub fn compiled_with_test_hooks() -> bool {
    cfg!(feature = "test-hooks")
}

/// A transient accept error should not stop the listen loop.
///
/// `ConnectionAborted` is a peer that vanished during accept.
/// `Interrupted` is a signal. `EMFILE` / `ENFILE` (24 / 23) clear once a
/// socket is closed. Anything else, including a closed listener, ends accept.
pub(crate) fn accept_error_ends_listen(err: &std::io::Error) -> bool {
    if matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::Interrupted
    ) {
        return false;
    }
    !matches!(err.raw_os_error(), Some(23 | 24))
}

/// How often a down `--connect` / `addnode` target is tried again.
#[cfg(not(any(test, feature = "test-hooks")))]
const REDIAL_INTERVAL: Duration = Duration::from_secs(2);
/// Crate tests and journeys. The node binary keeps the 2s pass.
#[cfg(any(test, feature = "test-hooks"))]
const REDIAL_INTERVAL: Duration = Duration::from_millis(250);

/// Bound on the TCP / SOCKS / SAM connect of an outbound dial, the same 8 s
/// as the IBD and tip-follow dials ([`crate::connect_timeout_for`] gives I2P
/// longer). Without it a proxy that accepts and never answers holds the
/// dial, and its [`crate::peers::DialInFlight`] mark, for good, so the
/// redial pass would never try that target again.
#[cfg(not(test))]
const OUTBOUND_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
#[cfg(test)]
const OUTBOUND_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[allow(clippy::too_many_arguments)] // call-site args stay unbundled
async fn prepare_outbound_session(
    peer: DialTarget,
    in_flight: Option<crate::peers::DialInFlight>,
    magic: Magic,
    local: SocketAddr,
    hub: Arc<ChainHub>,
    peers: Arc<PeerHub>,
    user_agent: String,
    follow_live: Arc<AtomicUsize>,
    typ: PeerConnType,
    dialer: crate::socks::Dialer,
) -> Result<PreparedOutbound, NetError> {
    hub.clock.note_connection_attempt();
    rbitcoin_log::debug!("{}", crate::peers::trying_connection_log(typ, &peer));
    // Until this returns, the redial pass and `addnode` treat `peer` as
    // already dialling. A queued dial was counted when it was queued.
    let _dialing = in_flight.unwrap_or_else(|| peers.dial_in_flight(&peer));
    let peer_net = peer.net_addr();
    if peers.net_discouraged(peer_net) {
        return Err(NetError::Protocol("discouraged"));
    }
    let connect = async {
        match &peer {
            DialTarget::Socket(_) => dialer.connect_net(peer_net).await,
            DialTarget::Domain { host, port } => dialer.connect_domain(host, *port).await,
        }
    };
    let stream = tokio::time::timeout(
        crate::connect_timeout_for(peer_net, OUTBOUND_CONNECT_TIMEOUT),
        connect,
    )
    .await
    .map_err(|_| NetError::Timeout)??;
    let peer_hint = peer.version_socket();
    let bind = stream.local_addr().unwrap_or(local);
    let height = hub.tip_height().map(|h| h as i32).unwrap_or(0);
    // Core adds CNode before VERSION. Provisional row so getpeerinfo is non-empty
    // during handshake (p2p_handshake self-connect wait_until + assert_debug_log).
    let provisional = peers.register_connecting_net(peer_hint, peer_net, bind, false, typ);
    let provisional_id = provisional.id;
    let handshake = connect_and_handshake_timed(
        HANDSHAKE_TIMEOUT,
        stream,
        magic,
        local,
        peer_hint,
        height,
        false,
        &user_agent,
        HandshakePolicy {
            hub: Some(hub.as_ref()),
            peers: Some(peers.as_ref()),
            session: Some(provisional.as_ref()),
            conn_type: typ,
        },
    )
    .await;
    let (ver, reader, writer, wire, tcp_shutdown) = match handshake {
        Ok(x) => x,
        Err(e) => {
            peers.unregister(provisional_id);
            return Err(e);
        }
    };
    let wants_addrv2 = provisional.wants_addrv2();
    let wtxid_relay = provisional.wtxid_relay();
    peers.unregister(provisional_id);
    let sess = peers.register_with_id_net(
        provisional_id,
        crate::peers::PeerEndpoint {
            addr: peer_hint,
            net: peer_net,
            addrbind: bind,
        },
        &ver,
        false,
        typ,
    );
    sess.mark_handshake_complete();
    if wants_addrv2 {
        sess.set_wants_addrv2();
    }
    if wtxid_relay {
        sess.set_wtxid_relay();
    }
    sess.note_recv("version", 100);
    sess.note_recv("verack", 0);
    sess.attach_tcp_shutdown(tcp_shutdown);
    if let Some(mp) = hub.mempool() {
        sess.set_inv_gen_floor(mp.next_accept_gen());
    }
    sess.attach_wire(wire);
    let id = sess.id;
    follow_live.fetch_add(1, Ordering::SeqCst);
    Ok(PreparedOutbound {
        peer_hint,
        magic,
        hub,
        peers,
        follow_live,
        reader,
        writer,
        sess,
        id,
    })
}

async fn run_prepared_outbound(prepared: PreparedOutbound) -> Result<(), NetError> {
    let tip_rx = prepared.hub.subscribe_tips();
    let meta = FollowSessionMeta {
        peer: Some(prepared.peer_hint),
        live: Some(prepared.follow_live),
        session: Some(prepared.sess),
    };
    let out = peer_session_with(
        prepared.reader,
        prepared.writer,
        prepared.magic,
        prepared.hub,
        tip_rx,
        meta,
    )
    .await;
    prepared.peers.unregister(prepared.id);
    out
}

#[allow(clippy::too_many_arguments)] // call-site args stay unbundled
async fn run_outbound_session_with_abort(
    peer: DialTarget,
    in_flight: Option<crate::peers::DialInFlight>,
    magic: Magic,
    local: SocketAddr,
    hub: Arc<ChainHub>,
    peers: Arc<PeerHub>,
    user_agent: String,
    follow_live: Arc<AtomicUsize>,
    typ: PeerConnType,
    ah_rx: tokio::sync::oneshot::Receiver<tokio::task::AbortHandle>,
    dialer: crate::socks::Dialer,
) -> Result<(), NetError> {
    if typ == PeerConnType::Feeler {
        let peer_addr = match peer {
            DialTarget::Socket(addr) => addr,
            DialTarget::Domain { .. } => {
                return Err(NetError::Encode("feeler requires ip:port target".into()))
            }
        };
        let peer_net = crate::NetAddr::Ip(peer_addr);
        if peers.net_discouraged(peer_net) {
            return Err(NetError::Protocol("discouraged"));
        }
        let stream = dialer.connect_net(peer_net).await?;
        let height = hub.tip_height().map(|h| h as i32).unwrap_or(0);
        return crate::peer::run_feeler(
            stream,
            magic,
            local,
            peer_addr,
            height,
            &user_agent,
            peers.as_ref(),
        )
        .await;
    }
    let prepared = prepare_outbound_session(
        peer,
        in_flight,
        magic,
        local,
        hub,
        peers,
        user_agent,
        follow_live,
        typ,
        dialer,
    )
    .await?;
    if let Ok(ah) = ah_rx.await {
        prepared.sess.set_session_abort(ah);
    }
    run_prepared_outbound(prepared).await
}

/// Resolve P2P message magic, including BIP325 custom-Signet derivation.
pub fn magic_for_params(params: &ChainParams) -> Magic {
    match params.signet_challenge.as_ref() {
        Some(challenge) => Magic::from_bytes(signet_magic(challenge.as_script())),
        None => Magic::from(params.network),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_retries_a_full_fd_table_and_stops_on_a_dead_listener() {
        let aborted = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "reset");
        let interrupted = std::io::Error::new(std::io::ErrorKind::Interrupted, "signal");
        let emfile = std::io::Error::from_raw_os_error(24);
        let enfile = std::io::Error::from_raw_os_error(23);
        let dead = std::io::Error::new(std::io::ErrorKind::InvalidInput, "closed");
        assert!(!accept_error_ends_listen(&aborted));
        assert!(!accept_error_ends_listen(&interrupted));
        assert!(!accept_error_ends_listen(&emfile));
        assert!(!accept_error_ends_listen(&enfile));
        assert!(accept_error_ends_listen(&dead));
    }

    #[test]
    fn magic_for_all_networks() {
        assert_eq!(
            magic_for_params(&ChainParams::mainnet()),
            Magic::from(bitcoin::Network::Bitcoin)
        );
        assert_eq!(
            magic_for_params(&ChainParams::testnet()),
            Magic::from(bitcoin::Network::Testnet)
        );
        assert_eq!(
            magic_for_params(&ChainParams::signet()),
            Magic::from(bitcoin::Network::Signet)
        );
        assert_eq!(magic_for_params(&ChainParams::regtest()), Magic::REGTEST);
    }

    #[test]
    fn custom_signet_uses_challenge_derived_magic() {
        use bitcoin::ScriptBuf;

        let challenge = ScriptBuf::from_bytes(vec![0x51]);
        let params = ChainParams::custom_signet(challenge, 60).unwrap();
        assert_eq!(
            magic_for_params(&params),
            Magic::from_bytes([0x54, 0xd2, 0x6f, 0xbd])
        );
    }

    /// A feeler does not use `prepare_outbound_session`. It still must not
    /// open a socket to an address that misbehaved.
    #[tokio::test]
    async fn feeler_does_not_dial_a_discouraged_address() {
        let peers = crate::peers::PeerHub::new();
        peers.set_mock_now(1_700_000_000);
        let addr = std::net::SocketAddr::from(([1, 2, 3, 4], 8333));
        peers.note_misbehavior_addr(addr.ip());
        let (dir, hub) = crate::chain::tiny_regtest_hub_labeled("feeler-discouraged");
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            run_outbound_session_with_abort(
                crate::peers::DialTarget::Socket(addr),
                None,
                Magic::REGTEST,
                std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                std::sync::Arc::new(hub),
                peers,
                "/rbitcoin:test/".into(),
                std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                crate::peers::PeerConnType::Feeler,
                rx,
                crate::socks::Dialer::Direct,
            ),
        )
        .await;
        let _ = std::fs::remove_dir_all(dir.path());
        let err = result
            .expect("a refused feeler returns before the connect")
            .unwrap_err();
        assert!(
            matches!(err, NetError::Protocol("discouraged")),
            "feeler dialed a discouraged address, got {err:?}"
        );
    }

    /// `--proxy` turns the DNS seeds into addr-fetch dials by name. The
    /// proxy resolves the name, so the node holds no address for it.
    #[tokio::test]
    async fn proxied_dns_seed_dials_by_name_through_socks() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = listener.local_addr().unwrap();
        let socks = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut greeting = [0u8; 2];
            s.read_exact(&mut greeting).await.unwrap();
            let mut methods = vec![0u8; greeting[1] as usize];
            s.read_exact(&mut methods).await.unwrap();
            if methods.contains(&0x02) {
                s.write_all(&[5, 0x02]).await.unwrap();
                let mut ulen = [0u8; 2];
                s.read_exact(&mut ulen).await.unwrap();
                let mut user = vec![0u8; ulen[1] as usize];
                s.read_exact(&mut user).await.unwrap();
                let mut plen = [0u8; 1];
                s.read_exact(&mut plen).await.unwrap();
                let mut pass = vec![0u8; plen[0] as usize];
                s.read_exact(&mut pass).await.unwrap();
                s.write_all(&[1, 0]).await.unwrap();
            } else {
                s.write_all(&[5, 0x00]).await.unwrap();
            }
            let mut req = [0u8; 5];
            s.read_exact(&mut req).await.unwrap();
            assert_eq!(req[3], 3, "ATYP domain");
            let mut host = vec![0u8; req[4] as usize];
            s.read_exact(&mut host).await.unwrap();
            let mut port = [0u8; 2];
            s.read_exact(&mut port).await.unwrap();
            s.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
            (String::from_utf8(host).unwrap(), u16::from_be_bytes(port))
        });
        let peers = crate::peers::PeerHub::new();
        let (dir, hub) = crate::chain::tiny_regtest_hub_labeled("proxied-dns-seed");
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_outbound_session_with_abort(
                crate::peers::DialTarget::Domain {
                    host: "seed.example".into(),
                    port: 8333,
                },
                None,
                Magic::REGTEST,
                std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                std::sync::Arc::new(hub),
                peers,
                "/rbitcoin:test/".into(),
                std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                crate::peers::PeerConnType::AddrFetch,
                rx,
                crate::socks::Dialer::socks(proxy, false),
            ),
        )
        .await;
        let _ = std::fs::remove_dir_all(dir.path());
        assert!(
            result
                .expect("the session ends when the proxy closes")
                .is_err(),
            "the proxy closed before VERSION"
        );
        assert_eq!(socks.await.unwrap(), ("seed.example".to_string(), 8333));
    }

    /// Dial and connect-retry block on a channel and a tick. Shutdown must
    /// wake them; the grace is only for a session that is still unwinding.
    #[tokio::test]
    async fn shutdown_finishes_without_the_dial_grace() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rbitcoin-shutdown-grace-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        let node = P2PNode::start_outbound_only(
            Query::open_or_create_tiny(&dir).unwrap(),
            ChainParams::regtest(),
            Milestone::NONE,
            "/rbitcoin:test/".into(),
            0,
            crate::socks::Dialer::Direct,
        )
        .await
        .unwrap();
        let t0 = tokio::time::Instant::now();
        node.shutdown().await;
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_millis(200),
            "dial and retry must observe shutdown, took {elapsed:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One datadir, two node lifetimes. The first is dropped without
    /// shutdown. The second binds loopback as with onion listen, adds a
    /// second listener, and shuts down with one session task that finishes
    /// in the grace window and one that would linger.
    #[tokio::test]
    async fn service_listen_shutdown() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = std::env::temp_dir().join(format!(
            "rbitcoin-p2p-listen-shutdown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let node = P2PNode::start(
            "127.0.0.1:0".parse().unwrap(),
            Query::open_or_create_tiny(&dir).unwrap(),
            ChainParams::regtest(),
            Milestone::NONE,
        )
        .await
        .unwrap();
        assert!(node.hub.shared_arc().is_some());
        let t0 = std::time::Instant::now();
        drop(node);
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "dropping the node must abort connect-retry without waiting out the runtime"
        );

        let mut node = P2PNode::start_with_dialer(
            "127.0.0.1:0".parse().unwrap(),
            Query::open_or_create_tiny(&dir).unwrap(),
            ChainParams::regtest(),
            Milestone::NONE,
            "/rbitcoin:0.1.0(onion)/".into(),
            crate::DEFAULT_MAX_INBOUND,
            crate::socks::Dialer::Direct,
        )
        .await
        .unwrap();
        assert_eq!(node.local_addr.ip(), std::net::Ipv4Addr::LOCALHOST);
        assert_ne!(node.local_addr.port(), 0);
        let extra = node
            .add_listen("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        assert_ne!(extra, node.local_addr);
        assert!(std::net::TcpStream::connect(extra).is_ok());

        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        node.tasks.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            flag.store(true, Ordering::SeqCst);
        }));
        node.tasks.push(tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }));
        let t0 = std::time::Instant::now();
        node.shutdown().await;
        assert!(
            done.load(Ordering::SeqCst),
            "shutdown lets a task that finishes within the grace window finish"
        );
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "shutdown must not wait out a 30s session task"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn p2p_outbound_only_dials_without_listener() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rbitcoin-outbound-only-{n}"));
        std::fs::create_dir_all(dir.join("seed")).unwrap();
        std::fs::create_dir_all(dir.join("follow")).unwrap();
        let qa = Query::open_or_create_tiny(dir.join("seed")).unwrap();
        let qb = Query::open_or_create_tiny(dir.join("follow")).unwrap();
        let params = ChainParams::regtest();
        let seeder = P2PNode::start_with_agent(
            "127.0.0.1:0".parse().unwrap(),
            qa,
            params.clone(),
            Milestone::NONE,
            "/rbitcoin:0.1.0(seed)/".into(),
            crate::DEFAULT_MAX_INBOUND,
        )
        .await
        .unwrap();
        let mut follower = P2PNode::start_outbound_only(
            qb,
            params,
            Milestone::NONE,
            "/rbitcoin:0.1.0(follow)/".into(),
            0,
            crate::socks::Dialer::Direct,
        )
        .await
        .unwrap();
        assert_eq!(
            follower.local_addr,
            "127.0.0.1:0".parse().unwrap(),
            "outbound-only must not bind a P2P port"
        );
        assert_eq!(follower.max_inbound, 0);
        let inbound_err = follower
            .peers
            .addconnection(seeder.local_addr, PeerConnType::Inbound)
            .unwrap_err();
        assert!(
            inbound_err.contains("inbound"),
            "addconnection inbound must refuse: {inbound_err}"
        );

        follower.follow_from(seeder.local_addr).await.unwrap();
        let mut linked = false;
        for _ in 0..100 {
            if follower.follow_live_count() >= 1 && !seeder.peers.snapshot().is_empty() {
                linked = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        seeder.shutdown().await;
        follower.shutdown().await;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(linked, "outbound-only follower must handshake the seeder");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn listen_onion_loopback_inbound_handshake() {
        use crate::peer::{connect_and_handshake_timed, HandshakePolicy};
        use bitcoin::p2p::Magic;
        use tokio::net::TcpStream;

        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rbitcoin-listen-onion-hs-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        let q = Query::open_or_create_tiny(&dir).unwrap();
        let node = P2PNode::start_with_dialer(
            "127.0.0.1:0".parse().unwrap(),
            q,
            ChainParams::regtest(),
            Milestone::NONE,
            "/rbitcoin:0.1.0(onion)/".into(),
            crate::DEFAULT_MAX_INBOUND,
            crate::socks::Dialer::Direct,
        )
        .await
        .unwrap();
        let stream = TcpStream::connect(node.local_addr).await.unwrap();
        let _hs = connect_and_handshake_timed(
            Duration::from_secs(5),
            stream,
            Magic::REGTEST,
            node.local_addr,
            node.local_addr,
            0,
            false,
            "/rbitcoin:test/",
            HandshakePolicy::plain(),
        )
        .await
        .unwrap();
        let mut inbound = false;
        for _ in 0..200 {
            if node.peers.snapshot().iter().any(|p| p.inbound) {
                inbound = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        node.shutdown().await;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            inbound,
            "loopback client must show as inbound in getpeerinfo"
        );
    }

    /// The 2 s redial pass must not stack a dial that is still connecting,
    /// and must not lose the target to one that never finishes. A SOCKS
    /// proxy that accepts and never answers holds the first dial in
    /// `connect_net`: later passes skip it until the connect times out, then
    /// dial again.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn redial_skips_a_connect_target_still_dialling() {
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let accepts = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&accepts);
        let stall = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((sock, _)) = proxy.accept().await {
                seen.fetch_add(1, Ordering::SeqCst);
                held.push(sock);
            }
        });
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rbitcoin-redial-in-flight-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        let node = P2PNode::start_outbound_only(
            Query::open_or_create_tiny(&dir).unwrap(),
            ChainParams::regtest(),
            Milestone::NONE,
            "/rbitcoin:0.1.0(redial)/".into(),
            0,
            crate::socks::Dialer::socks(proxy_addr, false),
        )
        .await
        .unwrap();
        node.peers
            .set_connect_addrs(vec![crate::NetAddr::Ip("127.0.0.1:18444".parse().unwrap())]);
        node.peers.redial_remembered();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while accepts.load(Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the first dial never reached the proxy"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        for _ in 0..3 {
            node.peers.redial_remembered();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            accepts.load(Ordering::SeqCst),
            1,
            "a dial still connecting must not be dialled again"
        );
        // The stalled connect times out and frees the target for the next pass.
        let deadline = std::time::Instant::now() + OUTBOUND_CONNECT_TIMEOUT * 3;
        while accepts.load(Ordering::SeqCst) < 2 {
            assert!(
                std::time::Instant::now() < deadline,
                "a connect that never finishes must time out and be redialled"
            );
            node.peers.redial_remembered();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        stall.abort();
        drop(node);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
