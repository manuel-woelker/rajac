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
//! - Success/failure status
//!
//! Class files are written to `[source_dir]/classes/` by default.

use rajac_compiler::{Compiler, CompilerConfig};
use rajac_base::file_path::FilePath;
use std::path::Path;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let source_dir = Path::new(&dir);

    let config = CompilerConfig {
        source_dir: FilePath::new(source_dir),
        target_dir: FilePath::new(source_dir.join("classes")),
    };
    let mut compiler = Compiler::new(config);

    if let Err(e) = compiler.compile_directory() {
        eprintln!("Compilation failed: {:?}", e);
        std::process::exit(1);
    }

    println!("Compiled successfully");
}
