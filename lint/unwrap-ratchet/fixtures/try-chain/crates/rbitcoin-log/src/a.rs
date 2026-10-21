fn f() {
    let bytes = [0u8; 4];
    let _ = bytes[..].try_into().ok().unwrap();
    let _ = bytes[..].try_into().as_ref().unwrap();
    let _ = bytes[..].try_into().ok()
        .unwrap();
}
