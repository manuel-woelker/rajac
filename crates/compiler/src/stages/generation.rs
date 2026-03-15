//! # Bytecode Generation Stage
//!
//! This module handles the final stage of the compilation pipeline: generating
//! Java bytecode class files from resolved Abstract Syntax Trees (ASTs).
//!
//! ## Purpose
//!
//! The generation stage is responsible for:
//! - Converting resolved ASTs into Java bytecode
//! - Creating proper class file structures with headers and metadata
//! - Generating constant pools and method bodies
//! - Adding debug information like source file attributes
//! - Writing class files to the target directory structure
//!
//! ## Implementation Details
//!
//! Uses the `ristretto_classfile` crate for bytecode generation:
//! - Follows JVM specification for class file format
//! - Handles constant pool management automatically
//! - Generates proper method descriptors and access flags
//! - Maintains compatibility with Java bytecode standards
//!
//! ## Output
//!
//! Produces `.class` files that can be:
//! - Executed by any JVM implementation
//! - Loaded by Java classloaders
//! - Debugged with standard Java tools
//! - Packaged into JAR files for distribution
//!
//! ## Usage
//!
//! This stage is typically called from the main compiler pipeline but can
//! be used independently for bytecode analysis or testing purposes.
//!
//! ```rust,no_run,ignore
//! use rajac_compiler::stages::generation;
//! use rajac_compiler::CompilationUnit;
//! use std::path::Path;
//!
//! let compilation_units = vec!/* ... */;
//! let target_dir = Path::new("target/classes");
//! let class_count = generation::generate_classfiles(&compilation_units, target_dir)?;
//! println!("Generated {} class files", class_count);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

/* 📖 # Why separate generation into its own stage?
Code generation is the final phase where ASTs are converted to bytecode.
This involves creating class files with proper attributes, constants,
and method implementations. Separating this stage allows for focused
testing of bytecode generation and potential optimization of the
output without affecting other compilation phases.
*/

use crate::CompilationUnit;
use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::shared_string::SharedString;
use rajac_bytecode::classfile::generate_classfiles_with_report as bytecode_generate_classfiles;
use rajac_diagnostics::{Annotation, Diagnostic, Diagnostics, Severity, SourceChunk, Span};
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::Path;

/// Generates class files from compilation units.
///
/// This function takes resolved compilation units and generates Java bytecode
/// class files for each class, interface, and enum found in the ASTs.
/// It handles the complete class file generation process including constant
/// pool creation, method body generation, and metadata attributes.
///
/// # Parameters
///
/// - `compilation_units` - A slice of compilation units containing resolved ASTs
/// - `target_dir` - The directory where class files should be written
///
/// # Returns
///
/// The total number of class files that were generated. This may differ
/// from the number of compilation units as each Java file can contain
/// multiple classes, interfaces, or enums.
///
/// # Errors
///
/// Returns an error if:
/// - Target directory cannot be created or accessed
/// - Class file generation fails due to invalid AST structure
/// - File I/O errors occur when writing class files
/// - Constant pool overflows or other bytecode limitations are hit
///
/// # Directory Structure
///
/// Class files are written according to Java package structure:
/// - `com/example/MyClass.java` → `com/example/MyClass.class`
/// - Nested classes get `$` in their names: `Outer$Inner.class`
/// - Package directories are created automatically
/// - Existing files are overwritten
///
/// # Examples
///
/// ```rust,no_run,ignore
/// use rajac_compiler::stages::generation;
/// use rajac_compiler::CompilationUnit;
/// use std::path::Path;
///
/// let compilation_units = vec!/* compilation units with resolved ASTs */;
/// let target_dir = Path::new("build/classes");
///
/// match generation::generate_classfiles(&compilation_units, target_dir) {
///     Ok(count) => {
///         println!("Successfully generated {} class files", count);
///     }
///     Err(e) => {
///         eprintln!("Bytecode generation failed: {:?}", e);
///     }
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Generated Files
///
/// Each compilation unit may generate multiple class files:
/// - Top-level classes: `MyClass.class`
/// - Nested classes: `Outer$Inner.class`
/// - Anonymous classes: `Outer$1.class`, `Outer$2.class`
/// - Local classes: `Method$1LocalClass.class`
/// - Interfaces and enums follow the same naming pattern
#[instrument(
    name = "compiler.phase.generation",
    skip(compilation_units, type_arena, symbol_table, target_dir),
    fields(
        compilation_units = compilation_units.len(),
        target_dir = %target_dir.display()
    )
)]
pub fn generate_classfiles(
    compilation_units: &mut [CompilationUnit],
    type_arena: &rajac_types::TypeArena,
    symbol_table: &rajac_symbols::SymbolTable,
    target_dir: &Path,
) -> RajacResult<(usize, Diagnostics)> {
    let mut total_files = 0;
    let mut diagnostics = Diagnostics::new();

    for unit in compilation_units {
        let (count, unit_diagnostics) = emit_classfiles(
            &unit.ast,
            &unit.arena,
            type_arena,
            symbol_table,
            &unit.source_file,
            target_dir,
        )
        .with_context(|| {
            format!(
                "Failed to generate class files for source file '{}'",
                unit.source_file.as_str()
            )
        })?;
        unit.diagnostics.extend(unit_diagnostics.iter().cloned());
        diagnostics.extend(unit_diagnostics);
        total_files += count;
    }

    Ok((total_files, diagnostics))
}

/// Emits class files for a single compilation unit.
///
/// This internal function handles the generation of class files for one
/// compilation unit, including all nested and anonymous classes.
/// It also adds source file debugging information to each generated class.
///
/// # Parameters
///
/// - `ast` - The resolved Abstract Syntax Tree
/// - `arena` - The arena containing AST node allocations
/// - `source_file` - The original source file path for debug information
/// - `target_dir` - Directory where class files should be written
///
/// # Returns
///
/// The number of class files generated for this compilation unit.
///
/// # Errors
///
/// Returns an error if bytecode generation or file writing fails.
///
/// # Source File Attribute
///
/// Each generated class file includes a `SourceFile` attribute containing
/// the original source filename. This enables:
/// - Better stack traces in debuggers
/// - Proper error reporting at runtime
/// - Source-level debugging support
/// - IDE integration for debugging
#[instrument(
    name = "compiler.phase.generation.file",
    skip(ast, arena, type_arena, symbol_table, source_file, target_dir),
    fields(source_file = %source_file.as_str(), target_dir = %target_dir.display())
)]
fn emit_classfiles(
    ast: &rajac_ast::Ast,
    arena: &rajac_ast::AstArena,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &rajac_symbols::SymbolTable,
    source_file: &FilePath,
    target_dir: &Path,
) -> RajacResult<(usize, Diagnostics)> {
    let generated = bytecode_generate_classfiles(ast, arena, type_arena, symbol_table)?;
    let mut diagnostics = Diagnostics::new();
    for unsupported_feature in &generated.unsupported_features {
        diagnostics.add(Diagnostic {
            severity: Severity::Error,
            message: unsupported_feature.message.clone(),
            chunks: vec![source_chunk_for_marker(
                source_file,
                ast.source.as_str(),
                unsupported_feature.marker.as_str(),
            )],
        });
    }

    let mut class_files = generated.class_files;

    for class_file in &mut class_files {
        let source_file_attribute_index = class_file.constant_pool.add_utf8("SourceFile")?;
        let source_file_index = class_file
            .constant_pool
            .add_utf8(source_file.file_name().unwrap_or("unknown"))
            .with_context(|| {
                format!(
                    "Failed to add SourceFile attribute for source file '{}'",
                    source_file.as_str()
                )
            })?;
        class_file.attributes.push(Attribute::SourceFile {
            name_index: source_file_attribute_index,
            source_file_index,
        })
    }

    let classfile_count = class_files.len();

    for class_file in class_files {
        let class_name = class_file
            .constant_pool
            .try_get_class(class_file.this_class)
            .with_context(|| {
                format!(
                    "Failed to get generated class name from constant pool for source file '{}'",
                    source_file.as_str()
                )
            })?;

        let class_path = target_dir.join(format!("{}.class", class_name));

        if let Some(parent) = class_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create package directory '{}' for class '{}'",
                    parent.display(),
                    class_name
                )
            })?;
        }

        let mut bytes = Vec::new();
        class_file
            .to_bytes(&mut bytes)
            .with_context(|| format!("Failed to serialize generated class '{}'", class_name))?;
        fs::write(&class_path, &bytes).with_context(|| {
            format!(
                "Failed to write class file '{}' for class '{}'",
                class_path.display(),
                class_name
            )
        })?;
    }

    Ok((classfile_count, diagnostics))
}

fn source_chunk_for_marker(source_file: &FilePath, source: &str, marker: &str) -> SourceChunk {
    let offset = source.find(marker).unwrap_or(0);
    let (line, line_start, line_end) = line_bounds_for_offset(source, offset);
    let fragment = &source[line_start..line_end];
    let annotation_start = fragment.find(marker).unwrap_or(0);
    let annotation_end = annotation_start + marker.len().max(1);

    SourceChunk {
        path: source_file.clone(),
        fragment: SharedString::new(fragment),
        offset: line_start,
        line,
        annotations: vec![Annotation {
            span: Span(annotation_start..annotation_end),
            message: SharedString::new(""),
        }],
    }
}

fn line_bounds_for_offset(source: &str, offset: usize) -> (usize, usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    let line = source[..line_start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    (line, line_start, line_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::{collection, resolution};
    use rajac_diagnostics::Diagnostics;
    use rajac_lexer::Lexer;
    use rajac_parser::Parser;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generate_classfiles_reports_unsupported_generation_features() -> RajacResult<()> {
        let source = r#"
class Test {
    void run() {
        try {
        } finally {
        }
    }
}
"#;
        let (mut units, symbol_table) = resolved_units(source);
        let target_dir = unique_test_output_dir();

        let (class_count, diagnostics) = generate_classfiles(
            &mut units,
            symbol_table.type_arena(),
            &symbol_table,
            &target_dir,
        )?;

        assert_eq!(class_count, 1);
        assert_eq!(diagnostics.len(), 1);

        let diagnostic = diagnostics.iter().next().expect("missing diagnostic");
        assert_eq!(
            diagnostic.message.as_str(),
            "unsupported bytecode generation feature: try statements"
        );
        assert_eq!(diagnostic.chunks[0].line, 4);
        assert_eq!(diagnostic.chunks[0].fragment.as_str().trim(), "try {");
        assert_eq!(units[0].diagnostics.len(), 1);

        std::fs::remove_dir_all(&target_dir).ok();
        Ok(())
    }

    fn resolved_units(source: &str) -> (Vec<CompilationUnit>, rajac_symbols::SymbolTable) {
        let parse_result = Parser::new(Lexer::new(source, FilePath::new("Test.java")), source)
            .parse_compilation_unit();
        let mut units = vec![CompilationUnit {
            source_file: FilePath::new("Test.java"),
            ast: parse_result.ast,
            arena: parse_result.arena,
            diagnostics: Diagnostics::new(),
        }];
        let mut symbol_table = rajac_symbols::SymbolTable::new();
        collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .expect("collect symbols");
        resolution::resolve_identifiers(&mut units, &mut symbol_table);
        (units, symbol_table)
    }

    fn unique_test_output_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rajac-generation-test-{nonce}"))
    }
}
