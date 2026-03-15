//! # Parsing Stage
//!
//! This module handles the second stage of the compilation pipeline: converting
//! Java source files into Abstract Syntax Trees (ASTs).
//!
//! ## Purpose
//!
//! The parsing stage is responsible for:
//! - Reading Java source files from disk
//! - Converting source code into structured ASTs
//! - Creating compilation units with associated arenas
//! - Handling syntax errors and providing meaningful error messages
//! - Supporting parallel parsing for improved performance
//!
//! ## Implementation Details
//!
//! Uses the `rayon` crate for parallel processing:
//! - Each Java file is parsed independently on available threads
//! - Results are collected into a vector of compilation units
//! - Errors from individual files are aggregated and reported
//!
//! ## Output
//!
//! Produces `CompilationUnit` instances containing:
//! - The original file path for reference
//! - The parsed AST representing the code structure
//! - An arena containing all AST node allocations
//!
//! ## Usage
//!
//! This stage is typically called from the main compiler pipeline but can
//! be used independently for parsing analysis or testing purposes.
//!
//! ```rust,no_run,ignore
//! use rajac_compiler::stages::parsing;
//! use rajac_base::file_path::FilePath;
//!
//! let java_files = vec![
//!     FilePath::new("src/Main.java"),
//!     FilePath::new("src/Utils.java"),
//! ];
//! let compilation_units = parsing::parse_files(&java_files)?;
//! println!("Parsed {} compilation units", compilation_units.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

/* 📖 # Why separate parsing into its own stage?
Parsing is a critical compilation phase that converts source code
into Abstract Syntax Trees (ASTs). It's computationally intensive
and benefits from parallel processing. Separating this stage allows
for better testing of parsing logic and potential optimization
without affecting other compilation phases.
*/

use crate::CompilationUnit;
use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_parser::parse;
use rayon::prelude::*;
use std::fs;

/// Parses Java source files into compilation units containing ASTs.
///
/// This function takes a collection of file paths and parses each Java source
/// file into a compilation unit containing the AST and associated arena.
/// Parsing is performed in parallel using all available CPU cores.
///
/// # Parameters
///
/// - `java_files` - A slice of `FilePath` references pointing to Java source files
///
/// # Returns
///
/// A `Vec<CompilationUnit>` containing the parsed ASTs. Each compilation unit
/// includes the original file path, the parsed AST, and an arena for AST nodes.
///
/// # Errors
///
/// Returns an error if any file cannot be parsed, such as:
/// - File not found or permission denied when reading source files
/// - Syntax errors in the Java source code
/// - Invalid Unicode sequences in source files
/// - Memory allocation failures during parsing
///
/// The error will include context about which file failed and why.
///
/// # Examples
///
/// ```rust,no_run,ignore
/// use rajac_compiler::stages::parsing;
/// use rajac_base::file_path::FilePath;
///
/// let java_files = vec![
///     FilePath::new("src/Main.java"),
///     FilePath::new("src/Helper.java"),
/// ];
///
/// match parsing::parse_files(&java_files) {
///     Ok(units) => {
///         for unit in &units {
///             println!("Parsed: {}", unit.source_file.as_str());
///         }
///     }
///     Err(e) => {
///         eprintln!("Parsing failed: {:?}", e);
///     }
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Performance Notes
///
/// - Files are parsed in parallel using all available CPU cores
/// - Each file is read completely before parsing begins
/// - Memory usage scales with the number of files being parsed simultaneously
/// - Large files may benefit from being processed in smaller batches
///
/// # Parallel Processing
///
/// This function uses `rayon` for parallel execution:
/// - Each file is processed on an available thread
/// - Results are collected in the original order
/// - Errors from any thread cause the entire operation to fail
/// - Thread safety is handled by the underlying parser
#[instrument(name = "compiler.phase.parsing", skip(java_files), fields(files = java_files.len()))]
pub fn parse_files(java_files: &[FilePath]) -> RajacResult<Vec<CompilationUnit>> {
    java_files
        .par_iter()
        .map(parse_file)
        .collect::<RajacResult<Vec<_>>>()
}

#[instrument(
    name = "compiler.phase.parsing.file",
    skip(java_file),
    fields(source_file = %java_file.as_str())
)]
fn parse_file(java_file: &FilePath) -> RajacResult<CompilationUnit> {
    let source = fs::read_to_string(java_file.as_path())
        .with_context(|| format!("Failed to read source file '{}'", java_file.as_str()))?;
    let parse_result = parse(&source, java_file.clone());
    Ok(CompilationUnit {
        source_file: java_file.clone(),
        ast: parse_result.ast,
        arena: parse_result.arena,
        diagnostics: parse_result.diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_file;
    use rajac_base::file_path::FilePath;

    #[test]
    fn missing_source_file_error_includes_source_path() {
        let missing_file = FilePath::new("tests/fixtures/does-not-exist/Main.java");

        let error = parse_file(&missing_file).unwrap_err();
        let rendered = error.to_test_string();

        assert!(
            rendered
                .contains("Failed to read source file 'tests/fixtures/does-not-exist/Main.java'")
        );
    }
}
