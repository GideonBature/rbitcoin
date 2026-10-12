#![no_main]

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

use libfuzzer_sys::fuzz_target;
use rbitcoin_consensus::Milestone;
use rbitcoin_fuzz::tmp_dir;
use rbitcoin_fuzz::{check_diff_env, diff_regtest_params, store_reorg_round};
use rbitcoin_net::ChainHub;
use rbitcoin_query::Query;

struct Base {
    hub: ChainHub,
    store: PathBuf,
}

static STATE: Mutex<Option<Base>> = Mutex::new(None);
static COMPARISONS: AtomicU64 = AtomicU64::new(0);

fn harness_failure(what: &str) -> ! {
    rbitcoin_fuzz::harness_failure("STORE-REORG FUZZ HARNESS FAILURE", None, what)
}

fn note_comparison(k: u32) {
    rbitcoin_fuzz::note_comparison(None, &COMPARISONS, u64::from(k), "store-reorg");
}

fn open_base() -> Base {
    if std::env::var_os("RBITCOIN_IO").is_none() {
        std::env::set_var("RBITCOIN_IO", "fd");
    }
    let io = std::env::var("RBITCOIN_IO").ok();
    if let Err(e) = check_diff_env(None, io.as_deref()) {
        harness_failure(e);
    }
    let parent = tmp_dir("rbtc-store-reorg");
    let store = parent.join("store");
    let q = Query::open_or_create_tiny(&store).unwrap_or_else(|e| {
        harness_failure(&format!("query open: {e}"));
    });
    let hub = ChainHub::new(q, diff_regtest_params(), Milestone::NONE);
    hub.ensure_genesis()
        .unwrap_or_else(|e| harness_failure(&format!("genesis: {e}")));
    Base { hub, store }
}

fn fail(msg: String) -> ! {
    if msg == "oracle dead" || msg.starts_with("reopen:") {
        harness_failure(&msg);
    }
    panic!("store_reorg: {msg}");
}

fuzz_target!(|data: &[u8]| {
    let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let base = slot.take().unwrap_or_else(open_base);
    let Base { hub, store } = base;
    match store_reorg_round(hub, &store, data) {
        Ok((hub, k)) => {
            if k > 0 {
                note_comparison(k);
            }
            *slot = Some(Base { hub, store });
        }
        Err(msg) => fail(msg),
    }
});
