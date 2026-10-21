fn f() {
    let m = std::sync::Mutex::new(1);
    let _ = m.lock().unwrap();
    let _ = Some(1).unwrap();
}
