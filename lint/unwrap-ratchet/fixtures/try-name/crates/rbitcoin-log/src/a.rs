fn f() {
    let _ = not_try_into().unwrap();
    let _ = ensure_try_into().unwrap();
    let _ = foo_try_into::<[u8; 4]>().unwrap();
}
