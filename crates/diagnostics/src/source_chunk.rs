use crate::annotation::Annotation;
use rajac_base::file_path::FilePath;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChunk {
    pub path: FilePath,
    pub fragment: SharedString,
    pub offset: usize,
    pub line: usize,
    pub annotations: Vec<Annotation>,
}
