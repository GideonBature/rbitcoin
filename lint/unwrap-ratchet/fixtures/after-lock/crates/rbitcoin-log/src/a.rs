fn f() {
    let m = std::sync::Mutex::new(1);
    let _g = m.lock().unwrap();
    let x = Some(1).unwrap();
}
