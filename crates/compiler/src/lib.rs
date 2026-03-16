//! # Rajac Compiler
//!
//! This crate provides the main compiler implementation for the Rajac Java compiler.
//! It orchestrates the entire compilation process from source discovery to bytecode generation.
//!
//! ## Architecture
//!
//! The compiler follows a pipeline architecture with distinct stages:
//!
//! 1. **Discovery** - Finding Java source files in the source directory
//! 2. **Parsing** - Converting source code to Abstract Syntax Trees (ASTs)
//! 3. **Collection** - Building symbol tables from ASTs
//! 4. **Resolution** - Resolving identifiers and types
//! 5. **Attribute Analysis** - Performing semantic checks on resolved ASTs
//! 6. **Generation** - Emitting Java bytecode class files
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rajac_compiler::{Compiler, CompilerConfig};
//! use rajac_base::file_path::FilePath;
//!
//! let config = CompilerConfig {
//!     source_dirs: vec![FilePath::new("src")],
//!     target_dir: FilePath::new("target/classes"),
//! };
//! let compiler = Compiler::new(config);
//! let result = compiler.compile()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Key Components
//!
//! - [`Compiler`] - Main compiler orchestrator
//! - [`CompilerConfig`] - Configuration for compilation settings
//! - [`CompilationUnit`] - Represents a single compiled source file

mod compilation_result;
pub mod compiler;
mod stages;
pub mod statistics;

pub use compilation_result::CompilationResult;
pub use compiler::{
    CompilationUnit, Compiler, CompilerConfig, default_java_classpaths,
    verification_java_classpaths,
};
