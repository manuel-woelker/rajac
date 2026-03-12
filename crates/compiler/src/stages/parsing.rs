/* 📖 # Why separate parsing into its own stage?
Parsing is a critical compilation phase that converts source code
into Abstract Syntax Trees (ASTs). It's computationally intensive
and benefits from parallel processing. Separating this stage allows
for better testing of parsing logic and potential optimization
without affecting other compilation phases.
*/

use crate::CompilationUnit;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_parser::parse;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Parses Java source files into compilation units containing ASTs.
pub fn parse_files(java_files: &[PathBuf]) -> RajacResult<Vec<CompilationUnit>> {
    java_files
        .par_iter()
        .map(|java_file| {
            let source = fs::read_to_string(java_file).context("Failed to read source file")?;
            let parse_result = parse(&source);
            Ok(CompilationUnit {
                source_file: java_file.clone(),
                ast: parse_result.ast,
                arena: parse_result.arena,
            })
        })
        .collect::<RajacResult<Vec<_>>>()
}
