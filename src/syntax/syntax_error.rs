/// A reason a file has no usable syntax tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub start_offset: usize,
    pub end_offset: usize,
    pub message: std::borrow::Cow<'static, str>,
}
