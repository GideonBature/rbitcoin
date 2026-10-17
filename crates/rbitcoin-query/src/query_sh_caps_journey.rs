fn assert_unspent_newest_page_stops(
    q: &Query,
    quiet_sh: [u8; 32],
    tip_cb_txid: [u8; 32],
) {
    use crate::scripthash::HistoryOrder::NewestFirst;

    assert_eq!(
        sh_page(q, &quiet_sh, NewestFirst, None).unwrap(),
        [tip_cb_txid],
        "newest-first page is the tip create"
    );
}

fn assert_newest_page_leaves_slot_empty(q: &Query, sh: &[u8; 32]) {
    let view = q.pin_sh_chain_view().unwrap().expect("sh view");
    let page = crate::scripthash::HistoryFilter::esplora_chain_page(None);
    let mut slot = None;
    let rows = q
        .scripthash_history_filtered_slot_in(sh, &page, &mut slot, &view)
        .expect("newest page")
        .rows;
    assert!(!rows.is_empty());
    assert!(
        slot.is_none(),
        "a limited newest-first page must not fill the full join slot"
    );
}

fn sh_page(
    q: &Query,
    sh: &[u8; 32],
    order: crate::scripthash::HistoryOrder,
    after_txid: Option<[u8; 32]>,
) -> Result<Vec<[u8; 32]>, QueryError> {
    let filter = crate::scripthash::HistoryFilter {
        limit: Some(1),
        order,
        after_txid,
        ..crate::scripthash::HistoryFilter::open()
    };
    Ok(q.scripthash_history_filtered(sh, &filter)?
        .rows
        .iter()
        .map(|i| i.txid)
        .collect())
}

/// One scripthash with more creates than `--max-sh-creates`: the full join
/// refuses, a page that closes before the cap is still served in either
/// order and past a cursor, and the create count includes the write-behind
/// that has not reached the durable index yet.
#[test]
fn sh_history_caps() {
    use crate::scripthash::HistoryOrder::{HeightAsc, NewestFirst};

    let (dir, q) = temp_query("sh-history-caps");
    let sh = script_hash(&[0x51]);
    let probe_sh = script_hash(&[0x52]);
    let add_probe_output = |ta: &mut TxApply| {
        ta.tx.output_count += 1;
        ta.outputs.push(OutputRecord::unspent(1, vec![0x52]));
    };
    let add_quiet_output = |ta: &mut TxApply| {
        ta.tx.output_count += 1;
        ta.outputs.push(OutputRecord::unspent(1, vec![0x53]));
    };
    let mut prev = Fk::NULL;
    let mut parent = None;
    let mut cb_txids = Vec::new();
    for h in 0..5u32 {
        let (header, mut ta) = coinbase_block(h, prev, parent);
        add_probe_output(&mut ta);
        if h >= 1 {
            add_quiet_output(&mut ta);
        }
        parent = Some(header.hash);
        cb_txids.push(ta.tx.txid);
        prev = q.connect_block(Height(h), &header, &[ta]).unwrap();
    }
    let create0 = q.block_tx_fks(Height(0)).unwrap()[0];

    let (h5, mut cb5) = coinbase_block(5, prev, parent);
    add_probe_output(&mut cb5);
    add_quiet_output(&mut cb5);
    let tip_cb_txid = cb5.tx.txid;
    let quiet_sh = script_hash(&[0x53]);

    let mut spend = cb5.clone();
    spend.tx.output_count = 1;
    spend.outputs.truncate(1);
    spend.tx.txid[30] = 0x5e;
    spend.inputs = vec![InputRecord {
    prev_txid: cb_txids[0],
    parent: rbitcoin_store::InputParent::Prevout { create_fk: create0, vout: 0 },
    sequence: u32::MAX,
    script_sig: vec![],
    witness: vec![],
}];
    let spend_txid = spend.tx.txid;
    q.commit_class_a_only(&h5, &[cb5, spend]).unwrap();
    q.confirm_block(Height(5), &h5.hash).unwrap();
    assert_eq!(q.pending_sh_create_fks(&sh).len(), 2);
    assert_eq!(
        q.scripthash_create_count(&sh).unwrap(),
        7,
        "the create count includes the pending write-behind"
    );
    q.apply_sh_pending().unwrap();
    assert!(q.pending_sh_create_fks(&sh).is_empty());
    assert_eq!(q.scripthash_create_count(&sh).unwrap(), 7);

    q.set_max_sh_creates(2);
    for err in [
        q.scripthash_chain_stats(&sh).map(|_| ()).unwrap_err(),
        q.scripthash_history(&sh).map(|_| ()).unwrap_err(),
    ] {
        assert!(
            err.to_string() == "scripthash join exceeds --max-sh-creates (7 creates, cap 2)",
            "{err}"
        );
    }
    assert_eq!(
        sh_page(&q, &sh, HeightAsc, None).unwrap(),
        [cb_txids[0]],
        "an ascending page that closes before the cap is served"
    );
    assert_eq!(
        sh_page(&q, &sh, HeightAsc, Some(cb_txids[0])).unwrap(),
        [cb_txids[1]],
        "a full page past the cursor closes too"
    );

    assert_eq!(
        sh_page(&q, &probe_sh, HeightAsc, Some(cb_txids[3])).unwrap(),
        [cb_txids[4]],
        "a cursor deeper than the cap still returns its next row"
    );

    assert_eq!(
        sh_page(&q, &sh, NewestFirst, None).unwrap(),
        [spend_txid],
        "the newest row spends the oldest create"
    );
    assert_unspent_newest_page_stops(&q, quiet_sh, tip_cb_txid);

    let view = q.pin_sh_chain_view().unwrap().expect("sh view");
    let page = crate::scripthash::HistoryFilter::esplora_chain_page(None);
    let mut slot = None;
    let rows = q
        .scripthash_history_filtered_slot_in(&sh, &page, &mut slot, &view)
        .expect("paged history skips the unpaged cap")
        .rows;
    assert!(!rows.is_empty());
    assert!(
        slot.is_none(),
        "a paged miss must not fill the join the cap refuses"
    );
    let sums = q
        .scripthash_history_summary_filtered_slot_in(&sh, &page, &mut slot, &view)
        .expect("paged summary skips the unpaged cap")
        .rows;
    assert!(!sums.is_empty());
    assert!(slot.is_none());
    let err = q
        .scripthash_history_filtered_slot_in(
            &sh,
            &crate::scripthash::HistoryFilter::open(),
            &mut slot,
            &view,
        )
        .unwrap_err();
    assert!(
        err.to_string() == "scripthash join exceeds --max-sh-creates (7 creates, cap 2)",
        "{err}"
    );

    q.set_max_sh_creates(0);
    assert_eq!(q.scripthash_history(&sh).unwrap().len(), 7);
    assert_newest_page_leaves_slot_empty(&q, &sh);
    q.set_max_sh_creates(7);
    assert_newest_page_leaves_slot_empty(&q, &sh);
    let stats = q.scripthash_chain_stats(&sh).unwrap();
    assert_eq!((stats.funded_txo_count, stats.spent_txo_count), (7, 1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Newest-first pages stay on the page path above, at, and below the cap.
/// Same-height order is txid, a sibling output's spend is not a row, and the
/// page after the tip creates is the spend of an older coin.
#[test]
fn newest_page_ignores_cap_and_sibling_spends() {
    use crate::scripthash::HistoryOrder::NewestFirst;

    let (dir, q) = temp_query("sh-newest-page");
    let sh = script_hash(&[0x51]);
    let (h0, mut cb0) = coinbase_block(0, Fk::NULL, None);
    cb0.tx.output_count = 2;
    cb0.outputs.push(OutputRecord::unspent(1, vec![0x99]));
    let cb0_txid = cb0.tx.txid;
    let h0fk = q.connect_block(Height(0), &h0, &[cb0]).unwrap();
    let create0 = q.block_tx_fks(Height(0)).unwrap()[0];

    let (h1, cb1) = coinbase_block(1, h0fk, Some(h0.hash));
    let mut spend_a = cb1.clone();
    spend_a.tx.txid[31] = 0xee;
    spend_a.tx.output_count = 1;
    spend_a.outputs = vec![OutputRecord::unspent(1, vec![0x00])];
    spend_a.inputs = vec![InputRecord {
        prev_txid: cb0_txid,
        parent: rbitcoin_store::InputParent::Prevout {
            create_fk: create0,
            vout: 0,
        },
        sequence: u32::MAX,
        script_sig: vec![],
        witness: vec![],
    }];
    let spend_txid = spend_a.tx.txid;
    let mut spend_sib = spend_a.clone();
    spend_sib.tx.txid[31] = 0x11;
    spend_sib.inputs[0].parent = rbitcoin_store::InputParent::Prevout {
        create_fk: create0,
        vout: 1,
    };
    let sib_txid = spend_sib.tx.txid;
    let h1fk = q
        .connect_block(Height(1), &h1, &[cb1, spend_a, spend_sib])
        .unwrap();

    let (h2, mut hi_txid) = coinbase_block(2, h1fk, Some(h1.hash));
    hi_txid.tx.txid[30] = 0xff;
    let hi = hi_txid.tx.txid;
    let mut lo_txid = hi_txid.clone();
    lo_txid.tx.txid[30] = 0x10;
    let lo = lo_txid.tx.txid;
    q.connect_block(Height(2), &h2, &[hi_txid, lo_txid]).unwrap();

    let page = |after| sh_page(&q, &sh, NewestFirst, after).unwrap();
    assert_eq!(
        page(None),
        vec![hi],
        "same-height order is txid, not create-fk"
    );
    assert_eq!(page(Some(hi)), vec![lo]);
    assert_eq!(
        page(Some(lo)),
        vec![spend_txid],
        "the next page is the spend of the older coin"
    );

    let full = q.scripthash_history(&sh).unwrap();
    let mut newest: Vec<[u8; 32]> = full.iter().map(|i| i.txid).collect();
    newest.reverse();
    assert!(
        !newest.contains(&sib_txid),
        "sibling spend is not this script"
    );
    q.set_max_sh_creates(0);
    assert_eq!(page(None), vec![hi]);
    assert_eq!(
        q.scripthash_history_filtered(&sh, &crate::scripthash::HistoryFilter::esplora_chain_page(None))
            .unwrap()
            .rows
            .iter()
            .map(|i| i.txid)
            .collect::<Vec<_>>(),
        newest,
    );
    assert_newest_page_leaves_slot_empty(&q, &sh);
    q.set_max_sh_creates(100);
    assert_newest_page_leaves_slot_empty(&q, &sh);
    q.set_max_sh_creates(1);
    assert_newest_page_leaves_slot_empty(&q, &sh);
    assert_eq!(page(Some(lo)), vec![spend_txid]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Limit 2 at the tip is the tip create plus the spend of the oldest coin.
/// The create between them is a real row and is not on that page.
#[test]
fn newest_page_is_tip_events_not_creates_between() {
    use crate::scripthash::HistoryOrder::NewestFirst;

    let (dir, q) = temp_query("sh-newest-window");
    let sh = script_hash(&[0x51]);
    let (h0, cb0) = coinbase_block(0, Fk::NULL, None);
    let old_txid = cb0.tx.txid;
    let h0fk = q.connect_block(Height(0), &h0, &[cb0]).unwrap();
    let old = q.block_tx_fks(Height(0)).unwrap()[0];

    let (h1, cb1) = coinbase_block(1, h0fk, Some(h0.hash));
    let mid = cb1.tx.txid;
    let h1fk = q.connect_block(Height(1), &h1, &[cb1]).unwrap();

    let (h2, tip) = coinbase_block(2, h1fk, Some(h1.hash));
    let tip_txid = tip.tx.txid;
    let mut spend = tip.clone();
    spend.tx.txid[31] = 0xff;
    spend.tx.output_count = 1;
    spend.outputs = vec![OutputRecord::unspent(1, vec![0x00])];
    spend.inputs = vec![InputRecord {
        prev_txid: old_txid,
        parent: rbitcoin_store::InputParent::Prevout {
            create_fk: old,
            vout: 0,
        },
        sequence: u32::MAX,
        script_sig: vec![],
        witness: vec![],
    }];
    let spend_txid = spend.tx.txid;
    q.connect_block(Height(2), &h2, &[tip, spend]).unwrap();

    let filter = crate::scripthash::HistoryFilter {
        limit: Some(2),
        order: NewestFirst,
        ..crate::scripthash::HistoryFilter::open()
    };
    let rows = q.scripthash_history_filtered(&sh, &filter).unwrap().rows;
    let txids: Vec<[u8; 32]> = rows.iter().map(|r| r.txid).collect();
    assert_eq!(txids, vec![spend_txid, tip_txid]);
    assert!(!txids.contains(&mid), "a create between the tip events is not on the page");

    let view = q.pin_sh_chain_view().unwrap().expect("sh view");
    let sums = q
        .scripthash_history_summary_filtered_in(&sh, &filter, &view)
        .unwrap()
        .rows;
    assert_eq!(sums.len(), 2);
    assert_eq!(sums[0].txid, spend_txid);
    assert_eq!(sums[0].value, -50_0000_0000);
    assert_eq!(sums[1].txid, tip_txid);
    assert_eq!(sums[1].value, 50_0000_0000);
    assert_eq!(
        sh_page(&q, &sh, NewestFirst, Some(spend_txid)).unwrap(),
        vec![tip_txid]
    );

    q.set_max_sh_creates(1);
    assert_eq!(
        q.scripthash_history_filtered(&sh, &filter)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.txid)
            .collect::<Vec<_>>(),
        vec![spend_txid, tip_txid]
    );
    assert_newest_page_leaves_slot_empty(&q, &sh);
    let _ = std::fs::remove_dir_all(&dir);
}
