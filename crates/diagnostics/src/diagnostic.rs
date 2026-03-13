use crate::severity::Severity;
use crate::source_chunk::SourceChunk;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: SharedString,
    pub chunks: Vec<SourceChunk>,
}
