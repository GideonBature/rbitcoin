fn f() {
    let bytes = [0u8; 4];
    let _ = bytes[..].try_into().unwrap();
    let _ = Some(1).unwrap();
}
