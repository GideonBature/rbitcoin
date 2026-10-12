#![no_main]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use rbitcoin_fuzz::{
    compare_script_verify_one, mine_diff_pad, submit_pad_to_oracle, CompareOne, CoreChild, DiffPad,
    DIFF_MATURE_PAD_HEIGHT,
};

struct Base {
    core: CoreChild,
    pad: DiffPad,
    _hub: rbitcoin_net::ChainHub,
    _store: PathBuf,
}

static BASE: OnceLock<Base> = OnceLock::new();
static COMPARISONS: AtomicU64 = AtomicU64::new(0);
static ORACLE_DOWN_STREAK: AtomicU64 = AtomicU64::new(0);
const MAX_ORACLE_DOWN_STREAK: u64 = 20;

fn harness_failure(what: &str) -> ! {
    rbitcoin_fuzz::harness_failure(
        "SCRIPT-VERIFY-DIFFERENTIAL FUZZ HARNESS FAILURE",
        Some(COMPARISONS.load(Ordering::Relaxed)),
        what,
    )
}

fn note_comparison() {
    rbitcoin_fuzz::note_comparison(
        Some(&ORACLE_DOWN_STREAK),
        &COMPARISONS,
        1,
        "script-verify-differential",
    );
}

fn base() -> &'static Base {
    BASE.get_or_init(|| {
        let (hub, core, store) = rbitcoin_fuzz::open_diff_hub(
            "rbtc-script-verify-store",
            "rbtc-script-verify-core",
            harness_failure,
        );
        let pad =
            mine_diff_pad(&hub, DIFF_MATURE_PAD_HEIGHT).unwrap_or_else(|e| harness_failure(e));
        if let Err(e) = submit_pad_to_oracle(&core.rpc, &pad.bodies) {
            harness_failure(e);
        }
        Base {
            core,
            pad,
            _hub: hub,
            _store: store,
        }
    })
}

fuzz_target!(|data: &[u8]| {
    let b = base();
    match compare_script_verify_one(&b.core.rpc, b.pad.mature, &b.pad.tip, data) {
        CompareOne::NotABlock | CompareOne::Skipped => {
            ORACLE_DOWN_STREAK.store(0, Ordering::Relaxed);
        }
        CompareOne::Agreed { .. } => note_comparison(),
        CompareOne::Disagreed {
            ours, core, hex, ..
        } => {
            eprintln!("=== SCRIPT-VERIFY-DIFFERENTIAL FUZZ CONSENSUS DIVERGENCE ===");
            eprintln!("ours_accept={ours} core_accept={core}");
            eprintln!("tx_hex={hex}");
            panic!("script-verify-differential: ours={ours} core={core} hex={hex}");
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
