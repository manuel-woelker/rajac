use crate::severity::Severity;
use crate::source_chunk::SourceChunk;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub chunks: Vec<SourceChunk>,
}
