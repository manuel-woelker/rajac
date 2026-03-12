// Compiler crate library
// Contains the main Compiler struct and related functionality

pub mod compiler;
pub mod stages;

pub use compiler::{CompilationUnit, Compiler, CompilerConfig};

// Re-export stages for external use if needed
pub use stages::{collection, discovery, generation, parsing, resolution};
