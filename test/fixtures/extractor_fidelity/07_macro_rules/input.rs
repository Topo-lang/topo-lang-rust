// The macro_rules definition is an Item that the extractor does not walk
// into — it should be silently skipped, and the sibling function should be
// extracted normally.
macro_rules! unused_macro {
    () => { () };
}

pub fn sibling() -> i32 {
    100
}
