use crate::annotation::Annotation;
use rajac_base::shared_string::SharedString;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChunk {
    pub path: PathBuf,
    pub fragment: SharedString,
    pub offset: usize,
    pub line: usize,
    pub annotations: Vec<Annotation>,
}
