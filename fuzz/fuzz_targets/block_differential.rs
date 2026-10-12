#![no_main]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use rbitcoin_fuzz::{
    compare_one, diff_regtest_params, genesis_diff_tip, CompareOne, CoreChild, DiffTip,
};

struct Base {
    hub: rbitcoin_net::ChainHub,
    core: CoreChild,
    tip: std::sync::Mutex<DiffTip>,
    _store: PathBuf,
}

static BASE: OnceLock<Base> = OnceLock::new();
static COMPARISONS: AtomicU64 = AtomicU64::new(0);
static ORACLE_DOWN_STREAK: AtomicU64 = AtomicU64::new(0);
const MAX_ORACLE_DOWN_STREAK: u64 = 20;

fn harness_failure(what: &str) -> ! {
    rbitcoin_fuzz::harness_failure(
        "BLOCK-DIFFERENTIAL FUZZ HARNESS FAILURE",
        Some(COMPARISONS.load(Ordering::Relaxed)),
        what,
    )
}

fn note_comparison() {
    rbitcoin_fuzz::note_comparison(
        Some(&ORACLE_DOWN_STREAK),
        &COMPARISONS,
        1,
        "block-differential",
    );
}

fn base() -> &'static Base {
    BASE.get_or_init(|| {
        let (hub, core, store) =
            rbitcoin_fuzz::open_diff_hub("rbtc-diff-store", "rbtc-diff-core", harness_failure);
        let params = diff_regtest_params();
        Base {
            hub,
            core,
            tip: std::sync::Mutex::new(genesis_diff_tip(&params)),
            _store: store,
        }
    })
}

fuzz_target!(|data: &[u8]| {
    let b = base();
    let mut tip = b.tip.lock().unwrap_or_else(|e| e.into_inner());
    match compare_one(&b.hub, &mut tip, &b.core.rpc, data) {
        CompareOne::NotABlock | CompareOne::Skipped => {
            ORACLE_DOWN_STREAK.store(0, Ordering::Relaxed);
        }
        CompareOne::Agreed { .. } => note_comparison(),
        CompareOne::Disagreed {
            ours, core, hex, ..
        } => {
            eprintln!("=== BLOCK-DIFFERENTIAL FUZZ CONSENSUS DIVERGENCE ===");
            eprintln!("ours_accept={ours} core_accept={core}");
            eprintln!("block_hex={hex}");
            panic!("block-differential: ours={ours} core={core} hex={hex}");
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
