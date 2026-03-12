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
use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::classfile::generate_classfiles as bytecode_generate_classfiles;
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
pub fn generate_classfiles(
    compilation_units: &[CompilationUnit],
    target_dir: &Path,
) -> RajacResult<usize> {
    let mut total_files = 0;

    for unit in compilation_units {
        let count = emit_classfiles(&unit.ast, &unit.arena, &unit.source_file, target_dir)?;
        total_files += count;
    }

    Ok(total_files)
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
fn emit_classfiles(
    ast: &rajac_ast::Ast,
    arena: &rajac_ast::AstArena,
    source_file: &FilePath,
    target_dir: &Path,
) -> RajacResult<usize> {
    let mut class_files = bytecode_generate_classfiles(ast, arena)?;

    for class_file in &mut class_files {
        let source_file_attribute_index = class_file.constant_pool.add_utf8("SourceFile")?;
        let source_file_index = class_file
            .constant_pool
            .add_utf8(source_file.file_name().unwrap_or("unknown"))?;
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
            .context("Failed to get class name from constant pool")?;

        let class_path = target_dir.join(format!("{}.class", class_name));

        if let Some(parent) = class_path.parent() {
            fs::create_dir_all(parent).context("Failed to create package directory")?;
        }

        let mut bytes = Vec::new();
        class_file.to_bytes(&mut bytes)?;
        fs::write(&class_path, &bytes).context(format!(
            "Failed to write class file: {}",
            class_path.display()
        ))?;
    }

    Ok(classfile_count)
}
