#[cfg(test)]
fn foo<T>()
where
    T: Clone,
{
    let _ = Some(1).unwrap();
}
