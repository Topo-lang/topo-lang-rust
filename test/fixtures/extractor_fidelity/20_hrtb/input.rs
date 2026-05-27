pub fn map<F>(_f: F)
where
    F: for<'a> Fn(&'a u8) -> &'a u8,
{
}
