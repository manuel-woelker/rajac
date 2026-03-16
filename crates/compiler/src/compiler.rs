/* 📖 # Why restructure the compiler with stages?
The compiler now follows a clear pipeline architecture with distinct stages:
1. Discovery - finding source files
2. Parsing - converting to ASTs
3. Collection - building symbol tables
4. Resolution - resolving identifiers and types
5. Attribute analysis - semantic checks on resolved ASTs
6. Generation - emitting bytecode

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
//!     source_dirs: vec![FilePath::new("src/main/java")],
//!     target_dir: FilePath::new("target/classes"),
//! };
//! let compiler = Compiler::new(config);
//!
//! // Compile entire directory
//! let result = compiler.compile()?;
//!
//! // Or execute stages individually
//! let mut compiler = Compiler::new(CompilerConfig {
//!     source_dirs: vec![FilePath::new("src/main/java")],
//!     target_dir: FilePath::new("target/classes"),
//!     classpaths: Vec::new(),
//!     emit_timing_statistics: false,
//! });
//! compiler.discover_files()?;
//! compiler.parse_files()?;
//! compiler.collect_symbols()?;
//! compiler.resolve_identifiers();
//! compiler.analyze_attributes();
//! compiler.generate_classfiles()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_diagnostics::Diagnostics;
use rajac_symbols::SymbolTable;
use std::path::{Path, PathBuf};

use crate::compilation_result::CompilationResult;
use crate::stages::{attribute_analysis, collection, discovery, generation, parsing, resolution};
use crate::statistics::{CompilationPhase, CompilationStatistics};

/// Represents a single compilation unit containing a parsed source file.
///
/// A compilation unit is created for each Java source file and contains:
/// - The source file path for reference and error reporting
/// - The parsed Abstract Syntax Tree (AST) representing the code structure
/// - The arena containing all AST node allocations
/// - Diagnostics collected during parsing
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
    /// Diagnostics collected during parsing
    pub diagnostics: Diagnostics,
}

/// Configuration for the compiler specifying source directories and target directory.
///
/// This struct defines where the compiler should look for Java source files
/// and where it should output the generated class files.
///
/// # Fields
///
/// - `source_dirs` - List of directories containing Java source files to compile
/// - `target_dir` - Directory where compiled class files will be written
///
/// # Example
///
/// ```rust
/// use rajac_compiler::CompilerConfig;
/// use rajac_base::file_path::FilePath;
///
/// let config = CompilerConfig {
///     source_dirs: vec![
///         FilePath::new("src/main/java"),
///         FilePath::new("src/test/java"),
///     ],
///     target_dir: FilePath::new("build/classes"),
///     classpaths: Vec::new(),
///     emit_timing_statistics: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// List of directories containing Java source files
    pub source_dirs: Vec<FilePath>,
    /// Directory where class files will be generated
    pub target_dir: FilePath,
    /// List of classpath entries (jar files and directories) to load symbols from
    pub classpaths: Vec<FilePath>,
    /// Whether to emit compilation timing statistics
    /// Defaults to false for production use
    pub emit_timing_statistics: bool,
}

/// Discovers Java standard-library classpath entries for the current machine.
///
/// This prefers Java 8-style `rt.jar` when present and otherwise falls back to
/// Java 9+ `jmods`. The returned paths are ordered deterministically.
pub fn default_java_classpaths() -> Vec<FilePath> {
    let mut homes = Vec::new();

    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        homes.push(PathBuf::from(java_home));
    }

    homes.extend(
        [
            "/usr/lib/jvm/default-runtime",
            "/usr/lib/jvm/default",
            "/usr/lib/jvm/java-21-openjdk",
            "/usr/lib/jvm/java-17-openjdk",
            "/usr/lib/jvm/java-11-openjdk",
            "/usr/lib/jvm/java-8-openjdk",
        ]
        .into_iter()
        .map(PathBuf::from),
    );

    for home in homes {
        let classpaths = java_runtime_classpaths_from_home(&home);
        if !classpaths.is_empty() {
            return classpaths;
        }
    }

    Vec::new()
}

fn java_runtime_classpaths_from_home(java_home: &Path) -> Vec<FilePath> {
    let rt_jar_candidates = [
        java_home.join("jre/lib/rt.jar"),
        java_home.join("lib/rt.jar"),
    ];
    for candidate in rt_jar_candidates {
        if candidate.is_file() {
            return vec![FilePath::new(candidate)];
        }
    }

    let jmods_dir = java_home.join("jmods");
    if !jmods_dir.is_dir() {
        return Vec::new();
    }

    let Ok(read_dir) = std::fs::read_dir(&jmods_dir) else {
        return Vec::new();
    };

    let mut jmods = read_dir
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "jmod"))
        .collect::<Vec<_>>();
    jmods.sort();

    jmods.into_iter().map(FilePath::new).collect()
}

/// Main compiler orchestrator that manages the compilation pipeline.
///
/// The compiler coordinates all stages of compilation from source discovery
/// through bytecode generation. It maintains state across stages and provides
/// high-level control over the compilation process.
///
/// The compiler is consumed by [`Compiler::compile`], which returns a
/// [`CompilationResult`] containing the diagnostics and timing statistics for
/// the completed run.
///
/// # Architecture
///
/// The compiler follows a pipeline architecture with these stages:
/// 1. **Discovery** - Find all Java source files
/// 2. **Parsing** - Convert source to ASTs
/// 3. **Collection** - Build symbol tables
/// 4. **Resolution** - Resolve identifiers and types
/// 5. **Attribute Analysis** - Perform semantic checks on resolved ASTs
/// 6. **Generation** - Emit bytecode class files
///
/// # Usage Patterns
///
/// ## High-Level Compilation
///
/// Use [`compile()`] for the complete compilation process:
///
/// ```rust,no_run,ignore
/// # use rajac_compiler::{Compiler, CompilerConfig};
/// # use rajac_base::file_path::FilePath;
/// # let config = CompilerConfig {
/// #     source_dirs: vec![FilePath::new("src")],
/// #     target_dir: FilePath::new("target"),
/// #     classpaths: Vec::new(),
/// #     emit_timing_statistics: false,
/// # };
/// let compiler = Compiler::new(config);
/// let result = compiler.compile()?;
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
/// #     source_dirs: vec![FilePath::new("src")],
/// #     target_dir: FilePath::new("target"),
/// #     classpaths: Vec::new(),
/// #     emit_timing_statistics: false,
/// # };
/// let mut compiler = Compiler::new(config);
///
/// // Execute stages individually
/// compiler.discover_files()?;
/// compiler.parse_files()?;
/// compiler.collect_symbols()?;
/// compiler.resolve_identifiers();
/// compiler.analyze_attributes();
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
    /// Whether classpath symbols have already been loaded into the symbol table
    pub classpath_symbols_loaded: bool,
    /// Compilation statistics accumulated during the current run
    statistics: CompilationStatistics,
    /// Diagnostics collected during compilation
    diagnostics: Diagnostics,
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
    ///     source_dirs: vec![FilePath::new("src")],
    ///     target_dir: FilePath::new("target"),
    ///     classpaths: Vec::new(),
    ///     emit_timing_statistics: false,
    /// };
    /// let compiler = Compiler::new(config);
    /// ```
    pub fn new(config: CompilerConfig) -> Self {
        Compiler {
            symbol_table: SymbolTable::new(),
            compilation_units: Vec::new(),
            java_files: Vec::new(),
            config,
            classpath_symbols_loaded: false,
            statistics: CompilationStatistics::new(),
            diagnostics: Diagnostics::new(),
        }
    }

    /// Creates a new compiler instance with a prepopulated symbol table.
    ///
    /// This skips classpath symbol collection during compilation, which is useful
    /// when the classpath has already been parsed and should be reused.
    ///
    /// # Parameters
    ///
    /// - `config` - Compiler configuration specifying source and target directories
    /// - `symbol_table` - Prepopulated symbol table (typically with classpath symbols)
    ///
    /// # Returns
    ///
    /// A new `Compiler` instance ready for compilation operations.
    pub fn new_with_symbol_table(config: CompilerConfig, symbol_table: SymbolTable) -> Self {
        Compiler {
            symbol_table,
            compilation_units: Vec::new(),
            java_files: Vec::new(),
            config,
            classpath_symbols_loaded: true,
            statistics: CompilationStatistics::new(),
            diagnostics: Diagnostics::new(),
        }
    }

    /// Builds a symbol table from classpath entries.
    ///
    /// This is useful for reusing classpath symbols across multiple compiler
    /// invocations without re-reading the classpath each time.
    ///
    /// # Parameters
    ///
    /// - `classpaths` - List of classpath entries (jar files and directories)
    ///
    /// # Returns
    ///
    /// A populated `SymbolTable` containing classpath symbols.
    pub fn symbol_table_from_classpaths(classpaths: &[FilePath]) -> RajacResult<SymbolTable> {
        let mut symbol_table = SymbolTable::new();
        collection::collect_classpath_symbols(&mut symbol_table, classpaths)?;
        Ok(symbol_table)
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
    /// 5. **Attribute Analysis** - Perform semantic checks on resolved ASTs
    /// 6. **Generation** - Emit bytecode class files
    ///
    /// # Errors
    ///
    /// Returns an error if any stage fails, such as:
    /// - Unable to create target directory
    /// - Source file parsing errors
    /// - Symbol collection conflicts
    /// - Bytecode generation failures
    ///
    /// Semantic compilation errors are reported through the returned
    /// [`CompilationResult`]. When diagnostics with error severity are emitted,
    /// this method still returns `Ok(_)`, generates classfiles only for
    /// compilation units without errors, and leaves all diagnostics available
    /// for the caller to inspect.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dirs: vec![FilePath::new("src")],
    /// #     target_dir: FilePath::new("target"),
    /// #     classpaths: Vec::new(),
    /// #     emit_timing_statistics: false,
    /// # };
    /// let compiler = Compiler::new(config);
    /// let result = compiler.compile()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(
        name = "compiler.compile",
        skip(self),
        fields(
            source_dirs = self.config.source_dirs.len(),
            classpaths = self.config.classpaths.len(),
            target_dir = %self.config.target_dir.as_str()
        )
    )]
    pub fn compile(mut self) -> RajacResult<CompilationResult> {
        std::fs::create_dir_all(self.config.target_dir.as_path()).with_context(|| {
            format!(
                "Failed to create target directory '{}'",
                self.config.target_dir.as_str()
            )
        })?;

        // Stage 1: Discovery - Find Java files
        self.discover_files()?;
        if self.java_files.is_empty() {
            if self.config.emit_timing_statistics {
                self.statistics.print_table();
            }
            return Ok(self.into_result());
        }

        // Stage 2: Parse source files
        let java_files = std::mem::take(&mut self.java_files);

        self.statistics.begin_phase(CompilationPhase::Parse);
        self.compilation_units = parsing::parse_files(&java_files)?;
        self.statistics.end_phase(CompilationPhase::Parse);

        self.java_files = java_files;

        // Collect diagnostics from compilation units
        for unit in &self.compilation_units {
            self.diagnostics.extend(unit.diagnostics.iter().cloned());
        }

        if !self.classpath_symbols_loaded {
            // Stage 2b: Collect classpath symbols
            self.statistics
                .begin_phase(CompilationPhase::ClasspathCollect);
            collection::collect_classpath_symbols(&mut self.symbol_table, &self.config.classpaths)?;
            self.statistics
                .end_phase(CompilationPhase::ClasspathCollect);
            self.classpath_symbols_loaded = true;
        }

        // Stage 3: Collect symbols from compilation units
        self.statistics.begin_phase(CompilationPhase::Collection);
        self.collect_symbols()?;
        self.statistics.end_phase(CompilationPhase::Collection);

        // Stage 4: Resolution - Resolve identifiers and types
        self.statistics.begin_phase(CompilationPhase::Resolution);
        self.resolve_identifiers();
        self.statistics.end_phase(CompilationPhase::Resolution);

        // Stage 5: Attribute analysis - Semantic checks
        self.statistics
            .begin_phase(CompilationPhase::AttributeAnalysis);
        self.analyze_attributes();
        self.statistics
            .end_phase(CompilationPhase::AttributeAnalysis);

        // Stage 6: Generation - Emit bytecode
        self.statistics.begin_phase(CompilationPhase::Generation);
        self.generate_classfiles()?;
        self.statistics.end_phase(CompilationPhase::Generation);

        if self.config.emit_timing_statistics {
            self.statistics.print_table();
        }

        Ok(self.into_result())
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
    /// #     source_dirs: vec![FilePath::new("src")],
    /// #     target_dir: FilePath::new("target"),
    /// #     classpaths: Vec::new(),
    /// #     emit_timing_statistics: false,
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    ///
    /// println!("Found {} Java files", compiler.java_files.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(
        name = "compiler.discover_files",
        skip(self),
        fields(source_dirs = self.config.source_dirs.len())
    )]
    fn discover_files(&mut self) -> RajacResult<()> {
        self.java_files.clear();
        for source_dir in &self.config.source_dirs {
            let mut files =
                discovery::find_java_files(source_dir.as_path()).with_context(|| {
                    format!("Failed to discover Java files in '{}'", source_dir.as_str())
                })?;
            self.java_files.append(&mut files);
        }
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
    /// #     source_dirs: vec![FilePath::new("src")],
    /// #     target_dir: FilePath::new("target"),
    /// #     classpaths: Vec::new(),
    /// #     emit_timing_statistics: false,
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    ///
    /// println!("Collected symbols from {} compilation units", compiler.compilation_units.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(
        name = "compiler.collect_symbols",
        skip(self),
        fields(compilation_units = self.compilation_units.len())
    )]
    fn collect_symbols(&mut self) -> RajacResult<()> {
        collection::collect_compilation_unit_symbols(
            &mut self.symbol_table,
            &self.compilation_units,
        )
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
    /// #     source_dirs: vec![FilePath::new("src")],
    /// #     target_dir: FilePath::new("target"),
    /// #     classpaths: Vec::new(),
    /// #     emit_timing_statistics: false,
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// compiler.collect_symbols()?;
    /// compiler.resolve_identifiers();
    /// compiler.analyze_attributes();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(
        name = "compiler.resolve_identifiers",
        skip(self),
        fields(compilation_units = self.compilation_units.len())
    )]
    fn resolve_identifiers(&mut self) {
        resolution::resolve_identifiers(&mut self.compilation_units, &mut self.symbol_table);
    }

    /// Performs attribute analysis on resolved compilation units.
    ///
    /// This method executes only the attribute analysis stage of the
    /// compilation pipeline. The current implementation is a stub that reserves
    /// the integration point for future semantic analysis work.
    ///
    /// # Prerequisites
    ///
    /// Identifiers should be resolved first using [`resolve_identifiers()`].
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use rajac_compiler::{Compiler, CompilerConfig};
    /// # use rajac_base::file_path::FilePath;
    /// # let config = CompilerConfig {
    /// #     source_dirs: vec![FilePath::new("src")],
    /// #     target_dir: FilePath::new("target"),
    /// #     classpaths: Vec::new(),
    /// #     emit_timing_statistics: false,
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// compiler.collect_symbols()?;
    /// compiler.resolve_identifiers();
    /// compiler.analyze_attributes();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(
        name = "compiler.analyze_attributes",
        skip(self),
        fields(compilation_units = self.compilation_units.len())
    )]
    fn analyze_attributes(&mut self) {
        let diagnostics = attribute_analysis::analyze_attributes(
            &mut self.compilation_units,
            &mut self.symbol_table,
        );
        self.diagnostics.extend(diagnostics);
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
    /// #     source_dirs: vec![FilePath::new("src")],
    /// #     target_dir: FilePath::new("target"),
    /// #     classpaths: Vec::new(),
    /// #     emit_timing_statistics: false,
    /// # };
    /// let mut compiler = Compiler::new(config);
    /// compiler.discover_files()?;
    /// compiler.parse_files()?;
    /// compiler.collect_symbols()?;
    /// compiler.resolve_identifiers();
    /// compiler.analyze_attributes();
    /// let class_count = compiler.generate_classfiles()?;
    ///
    /// println!("Generated {} class files", class_count);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(
        name = "compiler.generate_classfiles",
        skip(self),
        fields(
            compilation_units = self.compilation_units.len(),
            target_dir = %self.config.target_dir.as_str()
        )
    )]
    fn generate_classfiles(&mut self) -> RajacResult<usize> {
        let result = generation::generate_classfiles(
            &mut self.compilation_units,
            self.symbol_table.type_arena(),
            &self.symbol_table,
            self.config.target_dir.as_path(),
        )?;
        self.diagnostics.extend(result.diagnostics);
        Ok(result.class_count)
    }
}

impl Compiler {
    fn into_result(self) -> CompilationResult {
        CompilationResult::new(self.diagnostics, self.statistics)
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
            source_dirs: Vec::new(),
            target_dir: FilePath::default(),
            classpaths: Vec::new(),
            emit_timing_statistics: false,
        })
    }
}
