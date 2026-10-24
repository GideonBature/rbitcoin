/// BlindBit oracle v1 routes as Dana's `spdk` reads them: one eligible tx
/// with two P2TR outputs at height 1, its 200,000-sat output spent at height 2.
#[tokio::test(flavor = "current_thread")]
async fn blindbit_v1_routes_serve_the_thin_tweak_index() {
    use bitcoin::hashes::{sha256, Hash};
    use std::str::FromStr;

    let (_dir, q) = temp_query("blindbit-v1");
    q.set_sptweaks_enabled(true, Height(0)).unwrap();
    let q = Arc::new(q);

    let (h0, cb0) = coinbase(0, Fk::NULL, None);
    let cb0_txid = cb0.tx.txid;
    let fk0 = q.connect_block(Height(0), &h0, &[cb0]).unwrap();
    q.put_sp_tweaks_block(Height(0), fk0, &[None]).unwrap();
    let cb0_fk = q.block_tx_fks(Height(0)).unwrap()[0];

    let key_small = [0x11u8; 32];
    let key_big = [0x22u8; 32];
    // An ineligible tx's P2TR output: in the new-utxos filter, never a tweak.
    let key_change = [0x33u8; 32];
    let p2tr = |k: &[u8; 32]| {
        let mut s = vec![0x51, 0x20];
        s.extend_from_slice(k);
        s
    };
    let mut elig_txid = [0u8; 32];
    elig_txid[0] = 0xe1;
    let (h1, cb1) = coinbase(1, fk0, Some(h0.hash));
    let elig = TxApply {
        tx: TxRecord {
            txid: elig_txid,
            version: 2,
            locktime: 0,
            input_count: 1,
            output_count: 2,
        },
        inputs: vec![InputRecord {
            prev_txid: cb0_txid,
            parent: rbitcoin_store::InputParent::Prevout {
                create_fk: cb0_fk,
                vout: 0,
            },
            sequence: u32::MAX,
            script_sig: vec![],
            witness: vec![vec![0u8; 64]],
        }],
        outputs: vec![
            OutputRecord::unspent(600, p2tr(&key_small)),
            OutputRecord::unspent(200_000, p2tr(&key_big)),
        ],
    };
    let fk1 = q.connect_block(Height(1), &h1, &[cb1, elig]).unwrap();
    let tweak = [0x03u8; 33];
    q.put_sp_tweaks_block(Height(1), fk1, &[None, Some(tweak)])
        .unwrap();
    let elig_fk = q.block_tx_fks(Height(1)).unwrap()[1];

    let mut spend_txid = [0u8; 32];
    spend_txid[0] = 0x5e;
    let (h2, cb2) = coinbase(2, fk1, Some(h1.hash));
    let spend = TxApply {
        tx: TxRecord {
            txid: spend_txid,
            version: 2,
            locktime: 0,
            input_count: 1,
            output_count: 2,
        },
        inputs: vec![InputRecord {
            prev_txid: elig_txid,
            parent: rbitcoin_store::InputParent::Prevout {
                create_fk: elig_fk,
                vout: 1,
            },
            sequence: u32::MAX,
            script_sig: vec![],
            witness: vec![vec![0u8; 64]],
        }],
        outputs: vec![
            OutputRecord::unspent(1_500, p2tr(&key_change)),
            OutputRecord::unspent(100_000, vec![0x51]),
        ],
    };
    let fk2 = q.connect_block(Height(2), &h2, &[cb2, spend]).unwrap();
    q.put_sp_tweaks_block(Height(2), fk2, &[None, None]).unwrap();

    let mp_dir = _dir.join("mp");
    std::fs::create_dir_all(&mp_dir).unwrap();
    let hub = rbitcoin_net::MempoolHub::open(&mp_dir, Arc::clone(&q)).unwrap();
    hub.set_relay_enabled(true);
    let cfg = EsploraConfig::with_network("127.0.0.1:0".parse().unwrap(), Network::Regtest);
    let handle = run_esplora(cfg, Arc::clone(&q), Some(Arc::clone(&hub)))
        .await
        .expect("listen");
    let addr = handle.local_addr;
    let get = |path: &str| {
        let path = format!("/blindbit/v1{path}");
        async move { http_get(addr, &path).await }
    };
    let get_json = |path: &str| {
        let path = format!("/blindbit/v1{path}");
        async move {
            let (st, body) = http_get(addr, &path).await;
            assert_eq!(st, 200, "{path}: {body}");
            serde_json::from_str::<Value>(&body).unwrap()
        }
    };
    let tweak_hex = rbitcoin_primitives::hex_encode(tweak);

    let info = get_json("/info").await;
    assert_eq!(info["network"], "regtest", "a Core chain name, as spdk parses it");
    assert_eq!(info["height"], 2);
    assert_eq!(info["tweaks_only"], false);
    assert_eq!(get_json("/block-height").await, json!({ "block_height": 2 }));
    assert_eq!(
        get_json("/block-hash/1").await,
        json!({ "block_hash": block_hash_hex(&h1.hash) })
    );
    assert_eq!(get("/block-hash/3").await.0, 404, "past the indexed height");
    assert_eq!(get("/block-hash/one").await.0, 400);

    // tweak-index is every eligible tx; dustLimit keeps one whose largest
    // P2TR output is at least the limit.
    assert_eq!(get_json("/tweak-index/1").await, json!([tweak_hex]));
    assert_eq!(
        get_json("/tweak-index/1?dustLimit=200000").await,
        json!([tweak_hex])
    );
    assert_eq!(get_json("/tweak-index/1?dustLimit=200001").await, json!([]));
    assert_eq!(get("/tweak-index/1?dustLimit=lots").await.0, 400);
    assert_eq!(get_json("/tweak-index/0").await, json!([]));

    // tweaks is cut-through: the 200,000-sat output is spent, so only the
    // 600-sat output counts toward the dust limit.
    assert_eq!(get_json("/tweaks/1").await, json!([tweak_hex]));
    assert_eq!(get_json("/tweaks/1?dustLimit=600").await, json!([tweak_hex]));
    assert_eq!(get_json("/tweaks/1?dustLimit=1000").await, json!([]));

    let utxos = get_json("/utxos/1").await;
    let rows = utxos.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{utxos}");
    for (row, (vout, key, value, spent)) in rows
        .iter()
        .zip([(0, key_small, 600, false), (1, key_big, 200_000, true)])
    {
        assert_eq!(row["txid"], block_hash_hex(&elig_txid));
        assert_eq!(row["vout"], vout);
        assert_eq!(row["value"], value);
        assert_eq!(
            row["scriptpubkey"],
            rbitcoin_primitives::hex_encode(p2tr(&key))
        );
        assert_eq!(row["block_height"], 1);
        assert_eq!(row["block_hash"], block_hash_hex(&h1.hash));
        assert_eq!(row["timestamp"], h1.timestamp);
        assert_eq!(row["spent"], spent, "{row}");
    }

    // Height 2 spends one P2TR output. Height 1 spends only the OP_TRUE
    // coinbase output, which is not P2TR.
    let mut pre = Vec::new();
    pre.extend_from_slice(&elig_txid);
    pre.extend_from_slice(&1u32.to_le_bytes());
    pre.extend_from_slice(&h2.hash);
    let short = &sha256::Hash::hash(&pre).to_byte_array()[..8];
    assert_eq!(
        get_json("/spent-index/2").await,
        json!({
            "block_hash": block_hash_hex(&h2.hash),
            "data": [rbitcoin_primitives::hex_encode(short)],
        })
    );
    assert_eq!(
        get_json("/spent-index/1").await,
        json!({ "block_hash": block_hash_hex(&h1.hash), "data": [] }),
        "an empty index is still an object"
    );

    // Filters decode as spdk reads them: rust-bitcoin `match_any`, keyed by
    // the `block_hash` in the same response.
    let matches = |filter: &Value, element: &[u8]| {
        assert_eq!(filter["filter_type"], 4);
        let hash = bitcoin::BlockHash::from_str(filter["block_hash"].as_str().unwrap()).unwrap();
        let data = rbitcoin_primitives::hex_decode(filter["data"].as_str().unwrap()).unwrap();
        bitcoin::bip158::BlockFilter::new(&data)
            .match_any(&hash, std::iter::once(element))
            .unwrap()
    };
    let new1 = get_json("/filter/new-utxos/1").await;
    assert_eq!(new1["block_height"], 1);
    assert_eq!(new1["block_hash"], block_hash_hex(&h1.hash));
    assert!(matches(&new1, &key_small));
    assert!(matches(&new1, &key_big));
    assert!(!matches(&new1, &key_change));
    let new2 = get_json("/filter/new-utxos/2").await;
    assert!(
        matches(&new2, &key_change),
        "every P2TR output, eligible or not"
    );
    assert_eq!(get_json("/tweak-index/2").await, json!([]));
    assert!(matches(&get_json("/filter/spent/2").await, short));
    assert!(!matches(&get_json("/filter/spent/1").await, short));
    assert_eq!(get("/filter/spent/3").await.0, 404);

    // forward-tx answers the txid as a JSON string, which spdk parses.
    let fwd = bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: bitcoin::OutPoint {
                txid: bitcoin::Txid::from_byte_array(spend_txid),
                vout: 1,
            },
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![bitcoin::TxOut {
            value: bitcoin::Amount::from_sat(99_000),
            script_pubkey: bitcoin::ScriptBuf::from_bytes(vec![0x51]),
        }],
    };
    let body = json!({ "data": bitcoin::consensus::encode::serialize_hex(&fwd) }).to_string();
    let (st, resp) = http_post(addr, "/blindbit/v1/forward-tx", body.as_bytes()).await;
    assert_eq!(st, 200, "{resp}");
    let txid: String = serde_json::from_str(&resp).unwrap();
    assert_eq!(txid, fwd.compute_txid().to_string());
    assert!(hub.contains(&fwd.compute_txid()));
    for bad in [&b"not json"[..], br#"{"data":"zz"}"#, br#"{"data":"00"}"#] {
        let (st, resp) = http_post(addr, "/blindbit/v1/forward-tx", bad).await;
        assert_eq!(st, 400, "{resp}");
        let err: Value = serde_json::from_str(&resp).unwrap();
        assert!(err["error"].is_string(), "{resp}");
    }

    // A reorg between the reads one answer is built from is "chain view
    // moved", never the new block's data under the old block's hash.
    use rbitcoin_esplora::blindbit::{
        block_sp_elements, still_confirmed, thin_rows, ServedBlock, CHAIN_MOVED,
    };
    let (fk_at_2, rec) = q.header_at_height(Height(2)).unwrap().unwrap();
    let served = ServedBlock {
        height: 2,
        header_fk: fk_at_2,
        hash: rec.hash,
        time: rec.timestamp,
    };
    assert!(block_sp_elements(&q, &served).is_ok());
    assert!(thin_rows(&q, &served, false).is_ok());
    q.disconnect_tip().unwrap();
    assert!(
        matches!(block_sp_elements(&q, &served), Err(rbitcoin_store::StoreError::Stale(m)) if m == CHAIN_MOVED),
        "a disconnected height is a moved view, not a 500"
    );
    let status = thin_rows(&q, &served, false).err().map(|r| r.status());
    assert_eq!(
        status,
        Some(axum::http::StatusCode::SERVICE_UNAVAILABLE),
        "an unindexed height whose block left is a moved view, not 404"
    );
    let (mut other, cb) = coinbase(2, fk1, Some(h1.hash));
    other.nonce = other.nonce.wrapping_add(1);
    other.hash = rbitcoin_store::block_header_hash(
        other.version,
        &h1.hash,
        &other.merkle_root,
        other.timestamp,
        other.bits,
        other.nonce,
    );
    let other_fk = q.connect_block(Height(2), &other, &[cb]).unwrap();
    q.put_sp_tweaks_block(Height(2), other_fk, &[None]).unwrap();
    for moved in [
        still_confirmed(&q, &served).err(),
        block_sp_elements(&q, &served).err(),
    ] {
        assert!(
            matches!(moved, Some(rbitcoin_store::StoreError::Stale(m)) if m == CHAIN_MOVED),
            "{moved:?}"
        );
    }
    let status = thin_rows(&q, &served, false).err().map(|r| r.status());
    assert_eq!(status, Some(axum::http::StatusCode::SERVICE_UNAVAILABLE));
}

/// Without `--sp-tweaks` the prefix is not mounted.
#[tokio::test(flavor = "current_thread")]
async fn blindbit_v1_routes_need_the_tweak_index() {
    let (_dir, q) = temp_query("blindbit-v1-off");
    let (h0, cb0) = coinbase(0, Fk::NULL, None);
    q.connect_block(Height(0), &h0, &[cb0]).unwrap();
    let cfg = EsploraConfig::with_network("127.0.0.1:0".parse().unwrap(), Network::Regtest);
    let handle = run_esplora(cfg, Arc::new(q), None)
        .await
        .expect("listen");
    let (st, _) = http_get(handle.local_addr, "/blindbit/v1/info").await;
    assert_eq!(st, 404);
    let (st, _) = http_get(handle.local_addr, "/blocks/tip/height").await;
    assert_eq!(st, 200, "the rest of the listener is unchanged");
}

/// One semaphore bounds every `/blindbit/v1/` route. Held asks fill it; the
/// next BlindBit ask waits while the rest of the listener still answers,
/// and runs once a slot frees.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blindbit_v1_routes_share_one_inflight_limit() {
    use rbitcoin_esplora::blindbit::BLINDBIT_MAX_INFLIGHT;

    let (_dir, q) = temp_query("blindbit-v1-limit");
    q.set_sptweaks_enabled(true, Height(0)).unwrap();
    let (h0, cb0) = coinbase(0, Fk::NULL, None);
    let fk0 = q.connect_block(Height(0), &h0, &[cb0]).unwrap();
    q.put_sp_tweaks_block(Height(0), fk0, &[None]).unwrap();
    let cfg = EsploraConfig::with_network("127.0.0.1:0".parse().unwrap(), Network::Regtest);
    let handle = run_esplora(cfg, Arc::new(q), None)
        .await
        .expect("listen");
    let addr = handle.local_addr;

    // A forward-tx whose body never arrives holds its slot in the extractor.
    let mut held = Vec::new();
    for _ in 0..BLINDBIT_MAX_INFLIGHT {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let req = "POST /blindbit/v1/forward-tx HTTP/1.1\r\nHost: 127.0.0.1\r\n\
                   Content-Type: application/json\r\nContent-Length: 64\r\n\r\n";
        stream.write_all(req.as_bytes()).await.unwrap();
        held.push(stream);
    }
    let quick = Duration::from_millis(300);
    let mut waited = false;
    for _ in 0..50 {
        if tokio::time::timeout(quick, http_get(addr, "/blindbit/v1/info"))
            .await
            .is_err()
        {
            waited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(waited, "a full BlindBit limit makes the next ask wait");
    let (st, _) = tokio::time::timeout(quick, http_get(addr, "/blocks/tip/height"))
        .await
        .expect("the rest of the listener is not behind the BlindBit limit");
    assert_eq!(st, 200);
    let tweaks = tokio::spawn(async move { http_get(addr, "/blindbit/v1/tweak-index/0").await });
    drop(held.pop());
    let (st, body) = tokio::time::timeout(Duration::from_secs(10), tweaks)
        .await
        .expect("a freed slot serves the waiting ask")
        .unwrap();
    assert_eq!(st, 200, "{body}");
}
