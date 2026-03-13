use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span(pub Range<usize>);
