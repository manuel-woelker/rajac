//! # Symbol Collection Stage
//!
//! This module handles the third stage of the compilation pipeline: building
//! comprehensive symbol tables from parsed Abstract Syntax Trees (ASTs).
//!
//! ## Purpose
//!
//! The collection stage is responsible for:
//! - Discovering all class, interface, and method declarations
//! - Building hierarchical symbol tables for name resolution
//! - Integrating symbols from classpath entries (JAR files and directories)
//! - Organizing symbols by package structure
//! - Preparing data structures for the resolution phase
//!
//! ## Implementation Details
//!
//! Symbol collection involves:
//! - Traversing AST nodes to find declarations
//! - Creating symbol entries with proper kinds and visibility
//! - Maintaining package hierarchy for scope resolution
//! - Loading symbols from configured classpath entries
//!
//! ## Output
//!
//! Populates a `SymbolTable` containing:
//! - Package-organized symbol hierarchy
//! - Class and interface declarations
//! - Method and field symbols (when implemented)
//! - Symbols from classpath entries (JAR files and directories)
//!
//! ## Usage
//!
//! This stage is typically called from the main compiler pipeline but can
//! be used independently for symbol analysis or testing purposes.
//!
//! ```rust,no_run,ignore
//! use rajac_compiler::stages::collection;
//! use rajac_compiler::CompilationUnit;
//! use rajac_symbols::SymbolTable;
//! use rajac_base::file_path::FilePath;
//!
//! let compilation_units = vec!/* ... */;
//! let classpaths = vec![FilePath::new("lib/")];
//! let mut symbol_table = SymbolTable::new();
//! collection::collect_symbols(&mut symbol_table, &compilation_units, &classpaths)?;
//! println!("Collected symbols for {} packages", symbol_table.package_count());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

/* 📖 # Why separate collection into its own stage?
The collection phase builds symbol tables from parsed ASTs.
This is where we discover all classes, methods, and fields
and populate the symbol table that will be used during resolution.
Separating this stage allows for focused testing of symbol table
construction and potential optimization of symbol discovery.
*/

use crate::CompilationUnit;
use rajac_ast::{Ast, AstArena, ClassKind};
use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::shared_string::SharedString;
use rajac_classpath::Classpath;
use rajac_symbols::{SymbolKind, SymbolTable};

/// Populates the symbol table with symbols from classpath entries.
///
/// This function loads symbols from JAR files and directories in parallel using rayon.
/// It can be run concurrently with parsing since it doesn't depend on parsed ASTs.
///
/// # Parameters
///
/// - `symbol_table` - Mutable reference to the symbol table to populate
/// - `classpaths` - List of classpath entries (jar files and directories) to load symbols from
///
/// # Returns
///
/// `Ok(())` if symbol collection succeeds, or an error if classpath entries cannot be loaded.
///
/// # Classpath Entries
///
/// Each classpath entry can be:
/// - A JAR file (`.jar`) - all classes are loaded into the symbol table
/// - A directory - all `.class` files are loaded into the symbol table
#[instrument(
    name = "compiler.phase.collection.classpath",
    skip(symbol_table, classpaths),
    fields(classpaths = classpaths.len())
)]
pub fn collect_classpath_symbols(
    symbol_table: &mut SymbolTable,
    classpaths: &[FilePath],
) -> RajacResult<()> {
    let mut classpath = Classpath::new();
    for classpath_entry in classpaths {
        let path = classpath_entry.as_path();
        if path.exists() {
            if path
                .extension()
                .is_some_and(|ext| ext == "jar" || ext == "jmod")
            {
                classpath.add_jar(path);
            } else if path.is_dir() {
                classpath.add_directory(path);
            }
        }
    }
    if !classpath.is_empty() {
        classpath
            .add_to_symbol_table(symbol_table)
            .with_context(|| {
                format!(
                    "Failed to collect symbols from classpath entries: {:?}",
                    classpaths
                )
            })?;
    }
    Ok(())
}

/// Populates the symbol table with symbols from compilation units.
///
/// This function processes parsed ASTs to extract user-defined symbols.
/// It should be called after parsing is complete.
///
/// # Parameters
///
/// - `symbol_table` - Mutable reference to the symbol table to populate
/// - `compilation_units` - Slice of compilation units containing parsed ASTs
#[instrument(
    name = "compiler.phase.collection",
    skip(symbol_table, compilation_units),
    fields(compilation_units = compilation_units.len())
)]
pub fn collect_compilation_unit_symbols(
    symbol_table: &mut SymbolTable,
    compilation_units: &[CompilationUnit],
) -> RajacResult<()> {
    for unit in compilation_units {
        populate_symbol_table(symbol_table, &unit.ast, &unit.arena, &unit.source_file);
    }
    Ok(())
}

/// Populates the symbol table with symbols from a single AST.
///
/// This internal function processes one compilation unit's AST to extract
/// class and interface declarations and add them to the symbol table.
/// It handles package structure and symbol kind classification.
///
/// # Parameters
///
/// - `symbol_table` - Mutable reference to the symbol table to populate
/// - `ast` - Reference to the AST containing declarations
/// - `arena` - Reference to the AST arena for node access
///
/// # Symbol Processing
///
/// For each class declaration in the AST:
/// - Extracts the simple name and kind (class/interface)
/// - Determines the package context from AST's package declaration
/// - Creates appropriate symbol entries in the symbol table
/// - Skips enum, record, and annotation declarations for now
///
/// # Package Handling
///
/// - Uses AST's package declaration if present
/// - Defaults to empty package (default package) if none specified
/// - Creates or retrieves the appropriate package in the symbol table
/// - All symbols from the file are added to that package
#[instrument(
    name = "compiler.phase.collection.file",
    skip(symbol_table, ast, arena, source_file),
    fields(source_file = %source_file.as_str())
)]
fn populate_symbol_table(
    symbol_table: &mut SymbolTable,
    ast: &Ast,
    arena: &AstArena,
    source_file: &FilePath,
) {
    let package_name = ast
        .package
        .as_ref()
        .map(|p| {
            p.name
                .segments
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_default();

    for class_id in &ast.classes {
        let class = arena.class_decl(*class_id);
        let name = class.name.name.clone();

        let kind = match class.kind {
            ClassKind::Class => SymbolKind::Class,
            ClassKind::Interface => SymbolKind::Interface,
            ClassKind::Enum | ClassKind::Record | ClassKind::Annotation => continue,
        };

        let class_type = if !package_name.is_empty() {
            rajac_types::ClassType::new(name.clone()).with_package(SharedString::new(&package_name))
        } else {
            rajac_types::ClassType::new(name.clone())
        };

        let _type_id = symbol_table.add_class(
            &package_name,
            &name,
            rajac_types::Type::class(class_type),
            kind,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::collect_classpath_symbols;
    use rajac_base::file_path::FilePath;
    use rajac_symbols::SymbolTable;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn classpath_errors_include_entry_paths() {
        let temp_dir = unique_temp_dir("collection_context");
        fs::create_dir_all(&temp_dir).unwrap();

        let jar_path = temp_dir.join("broken.jar");
        fs::write(&jar_path, "not a jar").unwrap();

        let mut symbol_table = SymbolTable::new();
        let error =
            collect_classpath_symbols(&mut symbol_table, &[FilePath::new(&jar_path)]).unwrap_err();
        let rendered = error.to_test_string();

        assert!(rendered.contains("Failed to collect symbols from classpath entries"));
        assert!(rendered.contains("broken.jar"));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rajac_{name}_{nanos}"))
    }
}
