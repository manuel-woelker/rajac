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
//! - Integrating runtime library symbols (from rt.jar)
//! - Organizing symbols by package structure
//! - Preparing data structures for the resolution phase
//!
//! ## Implementation Details
//!
//! Symbol collection involves:
//! - Traversing AST nodes to find declarations
//! - Creating symbol entries with proper kinds and visibility
//! - Maintaining package hierarchy for scope resolution
//! - Loading standard library symbols for built-in types
//!
//! ## Output
//!
//! Populates a `SymbolTable` containing:
//! - Package-organized symbol hierarchy
//! - Class and interface declarations
//! - Method and field symbols (when implemented)
//! - Runtime library symbols from Java standard library
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
//!
//! let compilation_units = vec!/* ... */;
//! let mut symbol_table = SymbolTable::new();
//! collection::collect_symbols(&mut symbol_table, &compilation_units)?;
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
use rajac_base::result::RajacResult;
use rajac_classpath::Classpath;
use rajac_symbols::{Symbol, SymbolKind, SymbolTable};
use std::path::Path;

/// Populates the symbol table with symbols from compilation units and runtime library.
///
/// This function performs comprehensive symbol collection by:
/// 1. Loading runtime library symbols from the JVM's rt.jar
/// 2. Processing all compilation units to extract user-defined symbols
/// 3. Organizing symbols by package hierarchy
///
/// # Parameters
///
/// - `symbol_table` - Mutable reference to the symbol table to populate
/// - `compilation_units` - Slice of compilation units containing parsed ASTs
///
/// # Returns
///
/// `Ok(())` if symbol collection succeeds, or an error if:
/// - Runtime library cannot be loaded or parsed
/// - Symbol table population encounters conflicts
/// - AST structure is invalid or corrupted
///
/// # Runtime Library Integration
///
/// Automatically attempts to load symbols from:
/// - `/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar` (OpenJDK 8)
/// - Provides access to standard Java classes (java.lang.*, java.util.*, etc.)
/// - Enables proper type resolution for built-in types
/// - Gracefully skips if rt.jar is not found
///
/// # Examples
///
/// ```rust,no_run,ignore
/// use rajac_compiler::stages::collection;
/// use rajac_compiler::CompilationUnit;
/// use rajac_symbols::SymbolTable;
///
/// let compilation_units = vec!/* parsed compilation units */;
/// let mut symbol_table = SymbolTable::new();
/// 
/// match collection::collect_symbols(&mut symbol_table, &compilation_units) {
///     Ok(()) => {
///         println!("Successfully collected symbols");
///         // Symbol table is now ready for resolution phase
///     }
///     Err(e) => {
///         eprintln!("Symbol collection failed: {:?}", e);
///     }
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Symbol Organization
///
/// Symbols are organized by:
/// - **Package** - Top-level organization (e.g., "com.example")
/// - **Class/Interface** - Type declarations within packages
/// - **Kind** - Distinguishes between classes and interfaces
/// - **Name** - Simple name without package qualification
pub fn collect_symbols(
    symbol_table: &mut SymbolTable,
    compilation_units: &[CompilationUnit],
) -> RajacResult<()> {
    // Add runtime library symbols (rt.jar)
    let rt_jar = Path::new("/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar");
    if rt_jar.exists() {
        let mut classpath = Classpath::new();
        classpath.add_jar(rt_jar);
        classpath.add_to_symbol_table(symbol_table)?;
    }

    // Add symbols from compilation units
    for unit in compilation_units {
        populate_symbol_table(symbol_table, &unit.ast, &unit.arena);
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
fn populate_symbol_table(symbol_table: &mut SymbolTable, ast: &Ast, arena: &AstArena) {
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

    let package = symbol_table.package(&package_name);

    for class_id in &ast.classes {
        let class = arena.class_decl(*class_id);
        let name = class.name.name.clone();
        let kind = match class.kind {
            ClassKind::Class => SymbolKind::Class,
            ClassKind::Interface => SymbolKind::Interface,
            ClassKind::Enum | ClassKind::Record | ClassKind::Annotation => continue,
        };
        package.insert(name.to_string(), Symbol::new(name, kind));
    }
}
