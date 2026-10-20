/// I2P b32 hostname from a SAM `DESTINATION=` / `{datadir}/i2p/p2p.priv` blob.
///
/// Same host `NetAddr::host_str` reports for that destination.
pub fn i2p_b32_from_dest(dest: &str) -> String {
    rbitcoin_net::i2p_addr_from_sam_b64(dest)
        .unwrap_or_else(|e| panic!("{e}"))
        .host_str()
}
