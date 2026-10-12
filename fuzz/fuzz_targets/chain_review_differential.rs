#![no_main]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use rbitcoin_fuzz::{
    check_diff_env, compare_chain_plan, plan_chain_shape, spawn_bitcoind_extra, tmp_dir,
    ChainReview, CoreChild,
};

static CORE: OnceLock<CoreChild> = OnceLock::new();
static COMPARISONS: AtomicU64 = AtomicU64::new(0);
static ORACLE_DOWN_STREAK: AtomicU64 = AtomicU64::new(0);
const MAX_ORACLE_DOWN_STREAK: u64 = 20;

fn harness_failure(what: &str) -> ! {
    rbitcoin_fuzz::harness_failure(
        "CHAIN-REVIEW FUZZ HARNESS FAILURE",
        Some(COMPARISONS.load(Ordering::Relaxed)),
        what,
    )
}

fn note_comparison() {
    rbitcoin_fuzz::note_comparison(Some(&ORACLE_DOWN_STREAK), &COMPARISONS, 1, "chain-review");
}

fn core() -> &'static CoreChild {
    CORE.get_or_init(|| {
        let io = std::env::var("RBITCOIN_IO").ok();
        if let Err(e) = check_diff_env(None, io.as_deref()) {
            harness_failure(e);
        }
        let bin = std::env::var("RBITCOIN_CORE_BITCOIND").unwrap_or_default();
        if bin.is_empty() {
            harness_failure("RBITCOIN_CORE_BITCOIND unset");
        }
        let dir = tmp_dir("rbtc-chain-review-core");
        // Same BIP34-off regtest the hub shapes use, so a duplicate coinbase is
        // not rejected as a height push before BIP30.
        spawn_bitcoind_extra(
            std::path::Path::new(&bin),
            &dir,
            &["-testactivationheight=bip34@100000000"],
        )
        .unwrap_or_else(|e| harness_failure(&e))
    })
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let shape = data[0];
    let plan = match plan_chain_shape(shape) {
        Ok(plan) => plan,
        Err(e) => harness_failure(&e),
    };
    match compare_chain_plan(&plan, &core().rpc) {
        Ok(ChainReview::Agree { .. }) => note_comparison(),
        Ok(ChainReview::Disagree { ours, core, detail }) => {
            eprintln!("=== CHAIN-REVIEW FUZZ CONSENSUS DIVERGENCE ===");
            eprintln!("ours_accept={ours} core_accept={core} shape={shape} {detail}");
            panic!("chain-review: ours={ours} core={core} shape={shape} {detail}");
        }
        Err(msg) if msg == "oracle dead" || msg == "oracle rpc" => {
            let n = ORACLE_DOWN_STREAK.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= MAX_ORACLE_DOWN_STREAK {
                harness_failure(&msg);
            }
        }
        Err(msg) => harness_failure(&msg),
    }
});
