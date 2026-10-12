#![no_main]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use libfuzzer_sys::fuzz_target;
use rbitcoin_fuzz::{
    compare_mempool_one, mine_diff_pad, submit_pad_to_oracle, CompareOne, CoreChild, DiffPad,
    DIFF_MATURE_PAD_HEIGHT,
};

struct Base {
    hub: rbitcoin_net::ChainHub,
    core: CoreChild,
    pad: DiffPad,
    _store: PathBuf,
}

static BASE: OnceLock<Base> = OnceLock::new();
static COMPARISONS: AtomicU64 = AtomicU64::new(0);
static ORACLE_DOWN_STREAK: AtomicU64 = AtomicU64::new(0);
const MAX_ORACLE_DOWN_STREAK: u64 = 20;

fn harness_failure(what: &str) -> ! {
    rbitcoin_fuzz::harness_failure(
        "MEMPOOL-DIFFERENTIAL FUZZ HARNESS FAILURE",
        Some(COMPARISONS.load(Ordering::Relaxed)),
        what,
    )
}

fn note_comparison() {
    rbitcoin_fuzz::note_comparison(
        Some(&ORACLE_DOWN_STREAK),
        &COMPARISONS,
        1,
        "mempool-differential",
    );
}

fn base() -> &'static Base {
    BASE.get_or_init(|| {
        let (hub, core, store) = rbitcoin_fuzz::open_diff_hub(
            "rbtc-mp-diff-store",
            "rbtc-mp-diff-core",
            harness_failure,
        );
        let mp = rbitcoin_net::MempoolHub::open(store.join("mp"), Arc::clone(&hub.query))
            .unwrap_or_else(|e| harness_failure(&format!("mempool: {e}")));
        mp.set_relay_enabled(true);
        if hub.attach_mempool(mp).is_err() {
            harness_failure("attach mempool");
        }
        let pad =
            mine_diff_pad(&hub, DIFF_MATURE_PAD_HEIGHT).unwrap_or_else(|e| harness_failure(e));
        if let Err(e) = submit_pad_to_oracle(&core.rpc, &pad.bodies) {
            harness_failure(e);
        }
        Base {
            hub,
            core,
            pad,
            _store: store,
        }
    })
}

fuzz_target!(|data: &[u8]| {
    let b = base();
    match compare_mempool_one(&b.hub, &b.pad.tip, &b.core.rpc, b.pad.mature, data) {
        CompareOne::NotABlock | CompareOne::Skipped => {
            ORACLE_DOWN_STREAK.store(0, Ordering::Relaxed);
        }
        CompareOne::Agreed { .. } => note_comparison(),
        CompareOne::Disagreed {
            ours, core, hex, ..
        } => {
            eprintln!("=== MEMPOOL-DIFFERENTIAL FUZZ CONSENSUS DIVERGENCE ===");
            eprintln!("ours_accept={ours} core_accept={core}");
            eprintln!("tx_hex={hex}");
            panic!("mempool-differential: ours={ours} core={core} hex={hex}");
        }
        CompareOne::Harness(msg) if msg == "oracle dead" => {
            let n = ORACLE_DOWN_STREAK.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= MAX_ORACLE_DOWN_STREAK {
                harness_failure(msg);
            }
        }
        CompareOne::Harness(msg) => harness_failure(msg),
    }
});
