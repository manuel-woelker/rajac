//! # Rajac Compiler Main Entry Point
//!
//! This module provides the command-line interface for the Rajac Java compiler.
//! It handles argument parsing, configuration setup, and orchestrates the
//! compilation process from start to finish.
//!
//! ## Usage
//!
//! ```bash
//! rajac-compiler [source_directory]
//! ```
//!
//! If no source directory is provided, it defaults to "ballpit".
//! The compiler currently supports a single source directory, but the
//! underlying configuration supports multiple source directories for future use.
//!
//! ## Examples
//!
//! ```bash
//! # Compile default directory (ballpit)
//! rajac-compiler
//!
//! # Compile specific directory
//! rajac-compiler src/main/java
//!
//! # Compile current directory
//! rajac-compiler .
//! ```
//!
//! ## Output
//!
//! The compiler prints:
//! - Number of Java files found and processed
//! - Number of class files generated
//! - Success/failure status derived from emitted diagnostics
//!
//! Class files are written to `[source_dir]/classes/` by default.

use rajac_base::file_path::FilePath;
use rajac_compiler::{Compiler, CompilerConfig};
use rajac_diagnostics::Severity;
use std::path::Path;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let source_dir = Path::new(&dir);

    let config = CompilerConfig {
        source_dirs: vec![FilePath::new(source_dir)],
        target_dir: FilePath::new(source_dir.join("classes")),
        classpaths: vec!["/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar".into()],
        emit_timing_statistics: false,
    };
    let mut compiler = Compiler::new(config);

    if let Err(e) = compiler.compile_directory() {
        eprintln!("Compilation failed: {:?}", e);
        std::process::exit(1);
    }

    if compiler
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        eprintln!("Compilation failed: error diagnostics were emitted");
        std::process::exit(1);
    }

    println!("Compiled successfully");
}
