use rbitcoin_test::i2p_b32_from_dest;

#[test]
#[should_panic(expected = "i2p destination base64 contains")]
fn i2p_b32_rejects_unknown_base64() {
    let _ = i2p_b32_from_dest("!!!");
}

#[test]
#[should_panic(expected = "i2p destination too short")]
fn i2p_b32_rejects_a_short_destination() {
    let _ = i2p_b32_from_dest("AQID");
}

#[test]
fn i2p_b32_matches_the_address_host() {
    // Same 387-byte identity and private suffix as `i2p_addr_hashes_public_destination_only`.
    let mut ident = vec![7u8; 387];
    ident[384] = 0;
    ident[385] = 0;
    ident[386] = 0;
    let mut privd = ident.clone();
    privd.extend_from_slice(&[9u8; 80]);
    let host = "cxroqjd4tx5c5ufkracpqnx4hjjsgdj4uwhagtifp6yo5ed3hjlq.b32.i2p";
    assert_eq!(
        i2p_b32_from_dest(&rbitcoin_net::i2p_sam::i2p_b64_encode(&ident)),
        host
    );
    assert_eq!(
        i2p_b32_from_dest(&rbitcoin_net::i2p_sam::i2p_b64_encode(&privd)),
        host
    );
}
