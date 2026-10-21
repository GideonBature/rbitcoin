fn f() {
    let bytes = [0u8; 4];
    let _ = bytes[..].try_into::<[u8; 4]>().unwrap();
    let _ = bytes[..].try_into::<u8>().unwrap();
}
