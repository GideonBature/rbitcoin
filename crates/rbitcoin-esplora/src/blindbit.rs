//! BlindBit oracle v1 read routes under `/blindbit/v1/` (#1119).
//!
//! The contract is the v1 client in Dana's `spdk` (`backends/blindbit-v1`),
//! not BlindBit's own server where they differ: `spent-index` is always an
//! object, and `info.network` is a Core arg name. Served only while
//! `--sp-tweaks` is on, and only through the thin index's next height.
//!
//! `dustLimit` keeps a tx whose largest P2TR output is `>= dustLimit`
//! (BlindBit's edge). It is not `--sp-tweaks-dust`. `tweaks` is cut-through:
//! confirmed-spent outputs are dropped first. `tweak-index` is every
//! eligible tx.

use crate::handlers::spawn_join;
use crate::server::{block_hash_hex, AppState};
use axum::extract::{Path, Query as UrlQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bitcoin::hashes::{sha256, Hash};
use rbitcoin_primitives::Height;
use rbitcoin_query::{Query, QueryError, ThinTweakRangeLimits, ThinTweakRow};
use rbitcoin_store::StoreError;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

/// Mount point of the v1 surface on the Esplora listener.
pub const BLINDBIT_V1_PREFIX: &str = "/blindbit/v1";

/// The v1 read routes, nested under [`BLINDBIT_V1_PREFIX`].
pub fn blindbit_routes() -> Router<AppState> {
    Router::new().nest(
        BLINDBIT_V1_PREFIX,
        Router::new()
            .route("/info", get(info))
            .route("/block-height", get(block_height))
            .route("/block-hash/{height}", get(block_hash))
            .route("/tweaks/{height}", get(tweaks))
            .route("/tweak-index/{height}", get(tweak_index))
            .route("/utxos/{height}", get(utxos))
            .route("/spent-index/{height}", get(spent_index))
            .route("/filter/new-utxos/{height}", get(filter_new_utxos))
            .route("/filter/spent/{height}", get(filter_spent))
            .route("/forward-tx", post(forward_tx)),
    )
}

#[derive(Deserialize)]
pub struct DustParam {
    #[serde(rename = "dustLimit")]
    dust_limit: Option<String>,
}

/// Last height every v1 route can answer: the confirmed tip, capped by the
/// thin tweak index. `None` while the index is off or holds no height.
pub fn served_height(query: &Query) -> Option<u32> {
    if !query.sptweaks_enabled() {
        return None;
    }
    let next = query.sptweaks_next_height()?.0;
    let indexed = next.checked_sub(1)?;
    if next <= query.sptweaks_origin().0 {
        return None;
    }
    Some(indexed.min(query.tip_height()?.0))
}

/// BlindBit `HighestValue >= dustLimit` over the P2TR outputs a row still has.
pub fn keeps_dust(row: &ThinTweakRow, dust_limit: u64) -> bool {
    row.p2tr
        .iter()
        .map(|&(_, _, value)| value)
        .max()
        .unwrap_or(0)
        >= dust_limit
}

/// First 8 bytes of `sha256(txid ‖ vout_le ‖ block_hash)`, all in internal
/// byte order: BlindBit v1 `BuildSpentUTXOIndex`, salted by the spending block.
pub fn spent_short_hash(prev_txid: &[u8; 32], vout: u32, block_hash: &[u8; 32]) -> [u8; 8] {
    let mut buf = [0u8; 68];
    buf[..32].copy_from_slice(prev_txid);
    buf[32..36].copy_from_slice(&vout.to_le_bytes());
    buf[36..].copy_from_slice(block_hash);
    let digest = sha256::Hash::hash(&buf).to_byte_array();
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn store_error(e: QueryError) -> Response {
    error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
}

fn not_indexed() -> Response {
    error(StatusCode::NOT_FOUND, "height not indexed")
}

#[allow(clippy::result_large_err)] // axum Response, as in handlers
fn parse_height(s: &str) -> Result<u32, Response> {
    s.parse()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "could not parse block height"))
}

#[allow(clippy::result_large_err)] // axum Response, as in handlers
fn parse_dust(p: &DustParam) -> Result<u64, Response> {
    match p.dust_limit.as_deref() {
        None => Ok(0),
        Some(s) => s
            .parse()
            .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid dustLimit parameter")),
    }
}

/// A served height and its confirmed header hash (internal order) and time.
#[allow(clippy::result_large_err)] // axum Response, as in handlers
fn served_block(query: &Query, raw: &str) -> Result<(u32, [u8; 32], u32), Response> {
    let h = parse_height(raw)?;
    if served_height(query).is_none_or(|top| h > top) {
        return Err(not_indexed());
    }
    match query.header_at_height(Height(h)) {
        Ok(Some((_, rec))) => Ok((h, rec.hash, rec.timestamp)),
        Ok(None) => Err(not_indexed()),
        Err(e) => Err(store_error(e)),
    }
}

/// Thin rows at a served height. The index starts at Taproot activation,
/// so an earlier height is not indexed.
#[allow(clippy::result_large_err)] // axum Response, as in handlers
fn thin_rows(query: &Query, h: u32, cut_through: bool) -> Result<Vec<ThinTweakRow>, Response> {
    let limits = ThinTweakRangeLimits {
        max_heights: 1,
        max_eligible: usize::MAX,
        cut_through,
    };
    match query.load_thin_tweaks_range(Height(h), limits) {
        Ok(mut batch) => match batch.pop() {
            Some((_, rows)) => Ok(rows),
            None => Err(not_indexed()),
        },
        Err(e) => Err(store_error(e)),
    }
}

pub async fn info(State(st): State<AppState>) -> Response {
    spawn_join(move || {
        Json(json!({
            // `spdk` parses this with `Network::from_core_arg`.
            "network": st.network.to_core_arg(),
            "height": served_height(&st.query).unwrap_or(0),
            "tweaks_only": false,
            "tweaks_full_basic": true,
            "tweaks_full_with_dust_filter": true,
            "tweaks_cut_through_with_dust_filter": true,
        }))
        .into_response()
    })
    .await
}

pub async fn block_height(State(st): State<AppState>) -> Response {
    spawn_join(move || match served_height(&st.query) {
        Some(h) => Json(json!({ "block_height": h })).into_response(),
        None => error(StatusCode::SERVICE_UNAVAILABLE, "no height indexed"),
    })
    .await
}

pub async fn block_hash(State(st): State<AppState>, Path(raw): Path<String>) -> Response {
    spawn_join(move || match served_block(&st.query, &raw) {
        Ok((_, hash, _)) => Json(json!({ "block_hash": block_hash_hex(&hash) })).into_response(),
        Err(r) => r,
    })
    .await
}

fn tweak_list(st: &AppState, raw: &str, dust: &DustParam, cut_through: bool) -> Response {
    let dust_limit = match parse_dust(dust) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let h = match served_block(&st.query, raw) {
        Ok((h, _, _)) => h,
        Err(r) => return r,
    };
    match thin_rows(&st.query, h, cut_through) {
        Ok(rows) => {
            let list: Vec<String> = rows
                .iter()
                .filter(|r| keeps_dust(r, dust_limit))
                .map(|r| rbitcoin_primitives::hex_encode(r.tweak))
                .collect();
            Json(list).into_response()
        }
        Err(r) => r,
    }
}

pub async fn tweaks(
    State(st): State<AppState>,
    Path(raw): Path<String>,
    UrlQuery(dust): UrlQuery<DustParam>,
) -> Response {
    spawn_join(move || tweak_list(&st, &raw, &dust, true)).await
}

pub async fn tweak_index(
    State(st): State<AppState>,
    Path(raw): Path<String>,
    UrlQuery(dust): UrlQuery<DustParam>,
) -> Response {
    spawn_join(move || tweak_list(&st, &raw, &dust, false)).await
}

/// Every P2TR output of every eligible tx, with `spent` from the
/// cut-through view of the same height.
fn utxos_json(st: &AppState, raw: &str) -> Response {
    let (h, hash, time) = match served_block(&st.query, raw) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let full = match thin_rows(&st.query, h, false) {
        Ok(rows) => rows,
        Err(r) => return r,
    };
    let live: HashSet<([u8; 32], u32)> = match thin_rows(&st.query, h, true) {
        Ok(rows) => rows
            .iter()
            .flat_map(|r| r.p2tr.iter().map(move |&(vout, _, _)| (r.txid, vout)))
            .collect(),
        Err(r) => return r,
    };
    let block_hash = block_hash_hex(&hash);
    let mut out: Vec<Value> = Vec::new();
    for r in &full {
        let txid = block_hash_hex(&r.txid);
        for &(vout, xonly, value) in &r.p2tr {
            let mut spk = String::with_capacity(68);
            spk.push_str("5120");
            spk.push_str(&rbitcoin_primitives::hex_encode(xonly));
            out.push(json!({
                "txid": txid,
                "vout": vout,
                "value": value,
                "scriptpubkey": spk,
                "block_height": h,
                "block_hash": block_hash,
                "timestamp": time,
                "spent": !live.contains(&(r.txid, vout)),
            }));
        }
    }
    Json(out).into_response()
}

pub async fn utxos(State(st): State<AppState>, Path(raw): Path<String>) -> Response {
    spawn_join(move || utxos_json(&st, &raw)).await
}

/// What the spdk scan reads from one block: the x-only key of every P2TR
/// output, and the short hash of every input whose prevout is P2TR.
pub struct BlockSpElements {
    pub new_keys: Vec<[u8; 32]>,
    pub spent: Vec<[u8; 8]>,
}

/// One index-window read of height `h`: the block's outputs, input edges,
/// and every parent it spends. Bounded by the block, not the chain.
pub fn block_sp_elements(
    query: &Query,
    h: u32,
    block_hash: &[u8; 32],
) -> Result<BlockSpElements, QueryError> {
    let heights = query.index_heights(h, h, Some(h))?;
    let window = query.read_index_window(&heights)?;
    let mut new_keys = Vec::new();
    let mut spent = Vec::new();
    for block in &window.blocks {
        for o in block.txs.iter().flat_map(|t| t.outs.iter()) {
            if is_p2tr(&o.script) {
                let mut key = [0u8; 32];
                key.copy_from_slice(&o.script[2..]);
                new_keys.push(key);
            }
        }
        for e in block.edges.iter().flatten().filter(|e| !e.parent.is_null()) {
            let prev = window
                .prevout(e.parent, e.vout)
                .ok_or(StoreError::Corrupt("invariant: blindbit prevout missing"))?;
            if !is_p2tr(&prev.script) {
                continue;
            }
            let txid =
                window
                    .txid(e.parent)
                    .filter(|t| *t != [0u8; 32])
                    .ok_or(StoreError::Corrupt(
                        "invariant: blindbit parent txid missing",
                    ))?;
            spent.push(spent_short_hash(&txid, e.vout, block_hash));
        }
    }
    Ok(BlockSpElements { new_keys, spent })
}

fn is_p2tr(script: &[u8]) -> bool {
    rbitcoin_store::scripthash::is_p2tr(script)
}

/// BIP158 GCS (P=19, M=784931) keyed by the block hash, as BlindBit v1
/// builds both filters. Clients test membership; a repeated element is
/// written once here and twice by BlindBit, which does not change a match.
pub fn sp_filter_bytes<'a>(
    block_hash: &[u8; 32],
    elements: impl Iterator<Item = &'a [u8]>,
) -> Result<Vec<u8>, QueryError> {
    Ok(rbitcoin_query::block_filter::encode_basic_filter(block_hash, elements)?.content)
}

#[allow(clippy::result_large_err)] // axum Response, as in handlers
fn sp_elements_at(st: &AppState, raw: &str) -> Result<(u32, [u8; 32], BlockSpElements), Response> {
    let (h, hash, _) = served_block(&st.query, raw)?;
    let elements = block_sp_elements(&st.query, h, &hash).map_err(store_error)?;
    Ok((h, hash, elements))
}

fn spent_index_json(st: &AppState, raw: &str) -> Response {
    let (_, hash, elements) = match sp_elements_at(st, raw) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let data: Vec<String> = elements
        .spent
        .iter()
        .map(rbitcoin_primitives::hex_encode)
        .collect();
    Json(json!({ "block_hash": block_hash_hex(&hash), "data": data })).into_response()
}

pub async fn spent_index(State(st): State<AppState>, Path(raw): Path<String>) -> Response {
    spawn_join(move || spent_index_json(&st, &raw)).await
}

/// BlindBit v1 `filter_type` for both filters.
pub const SP_FILTER_TYPE: u8 = 4;

fn filter_json(st: &AppState, raw: &str, spent: bool) -> Response {
    let (h, hash, elements) = match sp_elements_at(st, raw) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let bytes = if spent {
        sp_filter_bytes(&hash, elements.spent.iter().map(|s| s.as_slice()))
    } else {
        sp_filter_bytes(&hash, elements.new_keys.iter().map(|k| k.as_slice()))
    };
    match bytes {
        Ok(data) => Json(json!({
            "filter_type": SP_FILTER_TYPE,
            "block_height": h,
            "block_hash": block_hash_hex(&hash),
            "data": rbitcoin_primitives::hex_encode(&data),
        }))
        .into_response(),
        Err(e) => store_error(e),
    }
}

pub async fn filter_new_utxos(State(st): State<AppState>, Path(raw): Path<String>) -> Response {
    spawn_join(move || filter_json(&st, &raw, false)).await
}

pub async fn filter_spent(State(st): State<AppState>, Path(raw): Path<String>) -> Response {
    spawn_join(move || filter_json(&st, &raw, true)).await
}

#[derive(Deserialize)]
pub struct ForwardTx {
    data: String,
}

/// Broadcast `{"data": "<hex>"}`. The answer is the txid as a JSON string:
/// spdk parses the body as a `Txid`, so BlindBit's empty 200 reads as an error.
pub async fn forward_tx(State(st): State<AppState>, body: axum::body::Bytes) -> Response {
    let Some(mp) = st.mempool.clone() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "mempool not available");
    };
    let req: ForwardTx = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return error(StatusCode::BAD_REQUEST, "could not parse request"),
    };
    let raw = match rbitcoin_primitives::hex_decode(req.data.trim()) {
        Ok(r) => r,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid hex"),
    };
    let tx: bitcoin::Transaction = match bitcoin::consensus::deserialize(&raw) {
        Ok(t) => t,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid tx"),
    };
    match mp.accept_tx_async(tx).await {
        Ok(r) => {
            mp.mark_local_origin(r.txid);
            Json(block_hash_hex(&r.txid.to_byte_array())).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}
