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

/* 📖 # Why separate generation into its own stage?
Code generation is the final phase where ASTs are converted to bytecode.
This involves creating class files with proper attributes, constants,
and method implementations. Separating this stage allows for focused
testing of bytecode generation and potential optimization of the
output without affecting other compilation phases.
*/

mod diagnostics;
mod emit;
mod generation_result;

use crate::CompilationUnit;
use generation_result::GenerationResult;
use rajac_base::logging::instrument;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_diagnostics::Severity;
use std::path::Path;

pub(crate) use diagnostics::generation_diagnostics_for_unsupported_features;

/// Generates class files from compilation units.
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
) -> RajacResult<GenerationResult> {
    let mut result = GenerationResult::default();

    for unit in compilation_units {
        if compilation_unit_has_errors(unit) {
            continue;
        }

        let unit_result = emit::emit_classfiles(
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
        unit.diagnostics
            .extend(unit_result.diagnostics.iter().cloned());
        result.class_count += unit_result.class_count;
        result.diagnostics.extend(unit_result.diagnostics);
    }

    Ok(result)
}

fn compilation_unit_has_errors(unit: &CompilationUnit) -> bool {
    unit.diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::{collection, resolution};
    use rajac_base::file_path::FilePath;
    use rajac_diagnostics::Diagnostics;
    use rajac_lexer::Lexer;
    use rajac_parser::Parser;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generate_classfiles_supports_try_finally() -> RajacResult<()> {
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

        let result = generate_classfiles(
            &mut units,
            symbol_table.type_arena(),
            &symbol_table,
            &target_dir,
        )?;

        assert_eq!(result.class_count, 1);
        assert!(result.diagnostics.is_empty());
        assert!(units[0].diagnostics.is_empty());
        assert!(target_dir.join("Test.class").exists());

        std::fs::remove_dir_all(&target_dir).ok();
        Ok(())
    }

    #[test]
    fn generate_classfiles_skips_units_with_error_diagnostics() -> RajacResult<()> {
        let source = r#"
class Good {
    void run() {
    }
}
"#;
        let parse_result = Parser::new(Lexer::new(source, FilePath::new("Good.java")), source)
            .parse_compilation_unit();
        let mut units = vec![
            CompilationUnit {
                source_file: FilePath::new("Good.java"),
                ast: parse_result.ast,
                arena: parse_result.arena,
                diagnostics: Diagnostics::new(),
            },
            CompilationUnit {
                source_file: FilePath::new("Broken.java"),
                ast: rajac_ast::Ast::new("class Broken {}".into()),
                arena: rajac_ast::AstArena::new(),
                diagnostics: Diagnostics::new(),
            },
        ];
        units[1].diagnostics.add(rajac_diagnostics::Diagnostic {
            severity: Severity::Error,
            message: "synthetic error".into(),
            chunks: vec![],
        });

        let mut symbol_table = rajac_symbols::SymbolTable::new();
        collection::collect_compilation_unit_symbols(&mut symbol_table, &units)
            .expect("collect symbols");
        resolution::resolve_identifiers(&mut units, &mut symbol_table);
        let target_dir = unique_test_output_dir();

        let result = generate_classfiles(
            &mut units,
            symbol_table.type_arena(),
            &symbol_table,
            &target_dir,
        )?;

        assert_eq!(result.class_count, 1);
        assert!(target_dir.join("Good.class").exists());
        assert!(!target_dir.join("Broken.class").exists());

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
