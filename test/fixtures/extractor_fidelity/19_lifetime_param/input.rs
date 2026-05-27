pub struct Holder<'a, T: 'a> {
    value: &'a T,
}
