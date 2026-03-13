use crate::annotation::Annotation;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChunk {
    pub path: PathBuf,
    pub fragment: String,
    pub offset: usize,
    pub line: usize,
    pub annotations: Vec<Annotation>,
}
