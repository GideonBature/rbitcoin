use rbitcoin_query::soft_densify::*;

#[test]
fn assign_stop_clamps_to_confirm_window_and_fetched() {
    let stop = 2048u64;
    let over = 4096u64;
    let free = BQ_SOFT_FREE_BYTES;
    assert_eq!(
        soft_densify_band_hi(1, 1000, 100, Some(5.0), stop, Some(80)),
        1000,
        "under assign-stop: full densify_hi"
    );
    assert_eq!(
        soft_densify_band_hi(1, 1000, free + 1, Some(0.1), BQ_ASSIGN_STOP_BYTES, Some(80)),
        6,
        "over free / under 1 GiB still uses confirm window"
    );
    // Over stop: min(confirm_window_hi, fetched_hi, densify_hi); rate 5 → window 300.
    assert_eq!(
        soft_densify_band_hi(1, 1000, over, Some(5.0), stop, Some(80)),
        80,
        "fetched_hi below window → fetched_hi"
    );
    assert_eq!(
        soft_densify_band_hi(1, 1000, over, Some(5.0), stop, Some(500)),
        300,
        "fetched_hi above window → confirm window, not fetched_hi"
    );
    assert_eq!(
        soft_densify_band_hi(1, 50, over, Some(5.0), stop, Some(80)),
        50,
        "densify_hi below fetched and window → densify_hi"
    );
    assert_eq!(
        soft_densify_band_hi(1, 1000, over, None, stop, Some(80)),
        1,
        "rate cold → path_lo only, not full fetched"
    );
    assert_eq!(
        soft_densify_band_hi(1, 1000, over, Some(5.0), stop, None),
        0,
        "no fetched range → empty densify band"
    );
    assert!(!soft_assign_stopped(over, 0));
    assert!(!soft_assign_stopped(over, u64::MAX));
    assert!(soft_assign_stopped(over, stop));
}

#[test]
fn quarter_full_is_rate_window_or_assign_stop_or_ahead_count() {
    let stop = BQ_ASSIGN_STOP_BYTES;
    let quarter = stop / 4;
    assert!(!soft_ahead_quarter_full(0, quarter, None, stop));
    assert!(!soft_ahead_quarter_full(10, quarter - 1, None, stop));
    assert!(soft_ahead_quarter_full(1, quarter, None, stop));
    // The 100 MiB free floor is the densify horizon, not this byte budget.
    assert!(!soft_ahead_quarter_full(
        10,
        BQ_SOFT_FREE_BYTES / 4,
        None,
        stop
    ));
    assert!(!soft_ahead_quarter_full(10, quarter, None, u64::MAX));
    assert!(!soft_ahead_quarter_full(10, quarter, None, 0));
    // 5 blk/s → 300 blocks. Bytes are far under a quarter of 1 GiB.
    assert!(!soft_ahead_quarter_full(74, 74 * 80, Some(5.0), stop));
    assert!(soft_ahead_quarter_full(75, 75 * 80, Some(5.0), stop));
    // Rate cold: 1000 small bodies is a hole; 999 is still the frontier.
    let min = TIP_HOLE_MIN_AHEAD_BLOCKS;
    assert!(!soft_ahead_quarter_full(
        min - 1,
        u64::from(min - 1) * 80,
        None,
        stop
    ));
    assert!(soft_ahead_quarter_full(
        min,
        u64::from(min) * 80,
        None,
        stop
    ));
    // 100 blk/s → 6000-block window (quarter 1500). 1000 still races.
    assert!(!soft_ahead_quarter_full(
        min - 1,
        u64::from(min - 1) * 80,
        Some(100.0),
        stop
    ));
    assert!(soft_ahead_quarter_full(
        min,
        u64::from(min) * 80,
        Some(100.0),
        stop
    ));
}
