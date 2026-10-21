fn f() {
    let m = std::sync::Mutex::new(1);
    let _ = m.lock().unwrap();
    let bytes = [0u8; 4];
    let _ = bytes[..].try_into().unwrap();
}

fn g() {
    let m = std::sync::Mutex::new(1);
    let _ = m
        .lock()
        .unwrap();
    let bytes = [0u8; 4];
    let _ = bytes[..]
        .try_into()
        .unwrap();
}
