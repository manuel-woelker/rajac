use crate::span::Span;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub span: Span,
    pub message: SharedString,
}
