#[cfg(test)]
impl<T> Foo for Bar<T>
where
    T: Clone,
{
    fn t(&self) {
        let _ = Some(1).unwrap();
    }
}

#[cfg(test)]
impl<'a> Foo for Bar<'a>
where
    'a: 'a,
{
    fn t(&self) {
        let _ = Some(1).unwrap();
    }
}

fn prod() {
    let _ = Some(2).unwrap();
}
