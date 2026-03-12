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

use rajac_base::result::{RajacResult, ResultExt};
use rajac_symbols::SymbolTable;
use std::path::PathBuf;

pub use crate::stages::{collection, discovery, generation, parsing, resolution};

pub struct CompilationUnit {
    pub source_file: PathBuf,
    pub ast: rajac_ast::Ast,
    pub arena: rajac_ast::AstArena,
}

pub struct CompilerConfig {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
}

#[allow(dead_code)]
pub struct Compiler {
    pub config: CompilerConfig,
    pub java_files: Vec<PathBuf>,
    pub compilation_units: Vec<CompilationUnit>,
    pub symbol_table: SymbolTable,
}

impl Compiler {
    pub fn new(config: CompilerConfig) -> Self {
        Compiler {
            symbol_table: SymbolTable::new(),
            compilation_units: Vec::new(),
            java_files: Vec::new(),
            config,
        }
    }

    pub fn compile_directory(&mut self) -> RajacResult<()> {
        std::fs::create_dir_all(&self.config.target_dir)
            .context("Failed to create target directory")?;

        // Stage 1: Discovery - Find Java files
        self.java_files = discovery::find_java_files(&self.config.source_dir)?;
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
            generation::generate_classfiles(&self.compilation_units, &self.config.target_dir)?;

        println!(
            "Compiled {} Java files -> {} class files",
            self.java_files.len(),
            classfile_count
        );

        Ok(())
    }

    // Individual stage methods for testing and fine-grained control
    pub fn discover_files(&mut self) -> RajacResult<()> {
        self.java_files = discovery::find_java_files(&self.config.source_dir)?;
        Ok(())
    }

    pub fn parse_files(&mut self) -> RajacResult<()> {
        self.compilation_units = parsing::parse_files(&self.java_files)?;
        Ok(())
    }

    pub fn collect_symbols(&mut self) -> RajacResult<()> {
        collection::collect_symbols(&mut self.symbol_table, &self.compilation_units)
    }

    pub fn resolve_identifiers(&mut self) {
        resolution::resolve_identifiers(&mut self.compilation_units, &self.symbol_table);
    }

    pub fn generate_classfiles(&mut self) -> RajacResult<usize> {
        generation::generate_classfiles(&self.compilation_units, &self.config.target_dir)
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new(CompilerConfig {
            source_dir: PathBuf::new(),
            target_dir: PathBuf::new(),
        })
    }
}
