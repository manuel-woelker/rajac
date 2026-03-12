/* 📖 # Why restructure the compiler with stages?
The compiler now follows a clear pipeline architecture with distinct stages:
1. Discovery - finding source files
2. Parsing - converting to ASTs
3. Collection - building symbol tables
4. Resolution - resolving identifiers and types
5. Generation - emitting bytecode

This separation makes the code more maintainable, testable, and allows
for easier optimization of individual stages. Each stage has clear
responsibilities and well-defined inputs/outputs.
*/

//! # Main Compiler Implementation
//!
//! This module contains the core compiler implementation that orchestrates
//! the entire compilation process from source discovery to bytecode generation.
//!
//! ## Overview
//!
//! The [`Compiler`] struct is the main entry point for compilation operations.
//! It manages the compilation pipeline and coordinates between different stages.
//!
//! ## Key Types
//!
//! - [`Compiler`] - Main compiler orchestrator with pipeline management
//! - [`CompilerConfig`] - Configuration for source and target directories
//! - [`CompilationUnit`] - Represents a single compiled source file with its AST
//!
//! ## Example Usage
//!
//! ```rust,no_run,ignore
//! use rajac_compiler::{Compiler, CompilerConfig};
//! use rajac_base::file_path::FilePath;
//!
//! let config = CompilerConfig {
//!     source_dir: FilePath::new("src/main/java"),
//!     target_dir: FilePath::new("target/classes"),
//! };
//! let mut compiler = Compiler::new(config);
//!
//! // Compile entire directory
//! compiler.compile_directory()?;
//!
//! // Or execute stages individually
//! compiler.discover_files()?;
//! compiler.parse_files()?;
//! compiler.collect_symbols()?;
//! compiler.resolve_identifiers();
//! compiler.generate_classfiles()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::file_path::FilePath;
use rajac_symbols::SymbolTable;

use crate::stages::{collection, discovery, generation, parsing, resolution};

/// Represents a single compilation unit containing a parsed source file.
///
/// A compilation unit is created for each Java source file and contains:
/// - The source file path for reference and error reporting
/// - The Abstract Syntax Tree (AST) representing the parsed code
/// - The arena containing all AST nodes and their allocations
///
/// This structure allows the compiler to maintain context about which
/// file each AST originated from, which is essential for error reporting
/// and generating proper debug information.
#[derive(Debug)]
pub struct CompilationUnit {
    /// Path to the source file that produced this compilation unit
    pub source_file: FilePath,
    /// The parsed Abstract Syntax Tree
    pub ast: rajac_ast::Ast,
    /// Arena containing all AST node allocations
    pub arena: rajac_ast::AstArena,
}

/// Configuration for the compiler specifying source and target directories.
///
/// This struct defines where the compiler should look for Java source files
/// and where it should output the generated class files.
///
/// # Fields
///
/// - `source_dir` - Directory containing Java source files to compile
/// - `target_dir` - Directory where compiled class files will be written
///
/// # Example
///
/// ```rust
/// use rajac_compiler::CompilerConfig;
/// use rajac_base::file_path::FilePath;
///
/// let config = CompilerConfig {
///     source_dir: FilePath::new("src/main/java"),
///     target_dir: FilePath::new("build/classes"),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Directory containing Java source files
    pub source_dir: FilePath,
    /// Directory where class files will be generated
    pub target_dir: FilePath,
}

/// Main compiler orchestrator that manages the compilation pipeline.
///
/// The compiler coordinates all stages of compilation from source discovery
/// through bytecode generation. It maintains state across stages and provides
/// both high-level and granular control over the compilation process.
///
/// # Architecture
///
/// The compiler follows a pipeline architecture with these stages:
/// 1. **Discovery** - Find all Java source files
/// 2. **Parsing** - Convert source to ASTs
/// 3. **Collection** - Build symbol tables
/// 4. **Resolution** - Resolve identifiers and types
/// 5. **Generation** - Emit bytecode class files
///
/// # Usage Patterns
///
/// ## High-Level Compilation
///
/// Use [`compile_directory()`] for the complete compilation process:
///
/// ```rust,no_run,ignore
/// # use rajac_compiler::{Compiler, CompilerConfig};
/// # use rajac_base::file_path::FilePath;
/// # let config = CompilerConfig {
/// #     source_dir: FilePath::new("src"),
/// #     target_dir: FilePath::new("target"),
/// # };
/// let mut compiler = Compiler::new(config);
/// compiler.compile_directory()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ## Granular Control
///
/// Execute individual stages for testing or debugging:
///
/// ```rust,no_run,ignore
/// # use rajac_compiler::{Compiler, CompilerConfig};
/// # use rajac_base::file_path::FilePath;
/// # let config = CompilerConfig {
/// #     source_dir: FilePath::new("src"),
/// #     target_dir: FilePath::new("target"),
/// # };
/// let mut compiler = Compiler::new(config);
///
/// // Execute stages individually
/// compiler.discover_files()?;
/// compiler.parse_files()?;
/// compiler.collect_symbols()?;
/// compiler.resolve_identifiers();
/// let class_count = compiler.generate_classfiles()?;
///
/// println!("Generated {} class files", class_count);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[allow(dead_code)]
pub struct Compiler {
    /// Compiler configuration
    pub config: CompilerConfig,
    /// List of discovered Java source files
    pub java_files: Vec<FilePath>,
    /// Parsed compilation units with ASTs
    pub compilation_units: Vec<CompilationUnit>,
    /// Global symbol table for all compilation units
    pub symbol_table: SymbolTable,
}

impl Compiler {
    /// Creates a new compiler instance with the given configuration.
    ///
    /// # Parameters
    ///
    /// - `config` - Compiler configuration specifying source and target directories
    ///
    /// # Returns
    ///
    /// A new `Compiler` instance ready for compilation operations.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// let config = CompilerConfig {
    ///     source_dir: FilePath::new("src"),
    ///     target_dir: FilePath::new("target"),
    /// };
    /// let compiler = Compiler::new(config);
    /// ```
    pub fn new(config: CompilerConfig) -> Self {
        Compiler {
            symbol_table: SymbolTable::new(),
            compilation_units: Vec::new(),
            java_files: Vec::new(),
            config,
        }
    }

    /// Compiles all Java files in the configured source directory.
    ///
    /// This is the main entry point for compilation and executes the complete
    /// pipeline from source discovery through bytecode generation.
    ///
    /// # Pipeline Stages
    ///
    /// 1. **Discovery** - Find all Java files in source directory
    /// 2. **Parsing** - Convert source files to ASTs
    /// 3. **Collection** - Build symbol tables from ASTs
    /// 4. **Resolution** - Resolve identifiers and types
    /// 5. **Generation** - Emit bytecode class files
    ///
    /// # Errors
    ///
    /// Returns an error if any stage fails, such as:
    /// - Unable to create target directory
    /// - Source file parsing errors
    /// - Symbol collection conflicts
    /// - Bytecode generation failures
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dir: FilePath::new("src"),
    /// #     target_dir: FilePath::new("target"),
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.compile_directory()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn compile_directory(&mut self) -> RajacResult<()> {
        std::fs::create_dir_all(self.config.target_dir.as_path())
            .context("Failed to create target directory")?;

        // Stage 1: Discovery - Find Java files
        self.discover_files()?;
        if self.java_files.is_empty() {
            return Ok(());
        }

        // Stage 2: Parsing - Convert source to ASTs
        self.compilation_units = parsing::parse_files(&self.java_files)?;

        // Stage 3: Collection - Build symbol tables
        collection::collect_symbols(&mut self.symbol_table, &self.compilation_units)?;

        // Stage 4: Resolution - Resolve identifiers and types
        resolution::resolve_identifiers(&mut self.compilation_units, &self.symbol_table);

        // Stage 5: Generation - Emit bytecode
        let classfile_count =
            generation::generate_classfiles(&self.compilation_units, self.config.target_dir.as_path())?;

        println!(
            "Compiled {} Java files -> {} class files",
            self.java_files.len(),
            classfile_count
        );

        Ok(())
    }

    /// Discovers Java source files in the configured source directory.
    ///
    /// This method executes only the discovery stage of the compilation pipeline.
    /// It's useful for testing or when you need to inspect which files would
    /// be compiled without performing the full compilation.
    ///
    /// # Errors
    ///
    /// Returns an error if the source directory cannot be accessed or scanned.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dir: FilePath::new("src"),
    /// #     target_dir: FilePath::new("target"),
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// 
    /// println!("Found {} Java files", compiler.java_files.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn discover_files(&mut self) -> RajacResult<()> {
        self.java_files = discovery::find_java_files(self.config.source_dir.as_path())?;
        Ok(())
    }

    /// Parses discovered Java source files into compilation units.
    ///
    /// This method executes only the parsing stage of the compilation pipeline.
    /// It converts the discovered Java source files into Abstract Syntax Trees (ASTs)
    /// and creates compilation units for each file.
    ///
    /// # Prerequisites
    ///
    /// Files must be discovered first using [`discover_files()`] or by setting
    /// `java_files` directly.
    ///
    /// # Errors
    ///
    /// Returns an error if any source file cannot be parsed, such as:
    /// - Syntax errors in the source code
    /// - File I/O errors when reading source files
    /// - Invalid Java language constructs
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dir: FilePath::new("src"),
    /// #     target_dir: FilePath::new("target"),
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// 
    /// println!("Parsed {} compilation units", compiler.compilation_units.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn parse_files(&mut self) -> RajacResult<()> {
        self.compilation_units = parsing::parse_files(&self.java_files)?;
        Ok(())
    }

    /// Collects symbols from parsed compilation units into the symbol table.
    ///
    /// This method executes only the collection stage of the compilation pipeline.
    /// It analyzes the ASTs to build comprehensive symbol tables containing
    /// classes, methods, fields, and other declarations.
    ///
    /// # Prerequisites
    ///
    /// Files must be parsed first using [`parse_files()`] or by setting
    /// `compilation_units` directly.
    ///
    /// # Errors
    ///
    /// Returns an error if symbol collection encounters issues such as:
    /// - Duplicate symbol definitions
    /// - Invalid symbol declarations
    /// - Scope resolution problems
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dir: FilePath::new("src"),
    /// #     target_dir: FilePath::new("target"),
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// compiler.collect_symbols()?;
    /// 
    /// println!("Collected symbols from {} compilation units", compiler.compilation_units.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn collect_symbols(&mut self) -> RajacResult<()> {
        collection::collect_symbols(&mut self.symbol_table, &self.compilation_units)
    }

    /// Resolves identifiers and types in the compilation units.
    ///
    /// This method executes only the resolution stage of the compilation pipeline.
    /// It analyzes the ASTs and symbol tables to resolve all identifiers,
    /// type references, and method calls to their actual declarations.
    ///
    /// # Prerequisites
    ///
    /// Symbols must be collected first using [`collect_symbols()`].
    ///
    /// # Notes
    ///
    /// This method does not return an error but may emit warnings or
    /// store resolution errors within the compilation units for later reporting.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dir: FilePath::new("src"),
    /// #     target_dir: FilePath::new("target"),
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// compiler.collect_symbols()?;
    /// compiler.resolve_identifiers();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn resolve_identifiers(&mut self) {
        resolution::resolve_identifiers(&mut self.compilation_units, &self.symbol_table);
    }

    /// Generates bytecode class files from the resolved compilation units.
    ///
    /// This method executes only the generation stage of the compilation pipeline.
    /// It converts the resolved ASTs into Java bytecode and writes class files
    /// to the configured target directory.
    ///
    /// # Prerequisites
    ///
    /// Identifiers must be resolved first using [`resolve_identifiers()`].
    ///
    /// # Returns
    ///
    /// Returns the number of class files that were generated.
    ///
    /// # Errors
    ///
    /// Returns an error if bytecode generation fails, such as:
    /// - Invalid bytecode instructions
    /// - File I/O errors when writing class files
    /// - Constant pool overflow
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dir: FilePath::new("src"),
    /// #     target_dir: FilePath::new("target"),
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// compiler.collect_symbols()?;
    /// compiler.resolve_identifiers();
    /// let class_count = compiler.generate_classfiles()?;
    /// 
    /// println!("Generated {} class files", class_count);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn generate_classfiles(&mut self) -> RajacResult<usize> {
        generation::generate_classfiles(&self.compilation_units, self.config.target_dir.as_path())
    }
}

impl Default for Compiler {
    /// Creates a compiler instance with default configuration.
    ///
    /// The default configuration uses empty paths for both source and target
    /// directories. This is primarily useful for testing or when you plan
    /// to configure the compiler manually after creation.
    ///
    /// # Returns
    ///
    /// A new `Compiler` instance with empty source and target directories.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rajac_compiler::Compiler;
    ///
    /// let compiler = Compiler::default();
    /// // Note: You'll need to set the configuration before compilation
    /// ```
    fn default() -> Self {
        Self::new(CompilerConfig {
            source_dir: FilePath::default(),
            target_dir: FilePath::default(),
        })
    }
}
