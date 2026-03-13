use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub span: Span,
    pub message: String,
}
