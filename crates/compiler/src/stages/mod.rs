//! # Compiler Stages
//!
//! This module contains the individual stages of the compilation pipeline.
//! Each stage represents a distinct phase in the compilation process with
//! clear inputs and outputs, making the pipeline modular and testable.
//!
//! ## Pipeline Flow
//!
//! The stages are executed in the following order:
//!
//! 1. **Discovery** (`discovery`) - Scans source directories for Java files
//! 2. **Parsing** (`parsing`) - Converts source code to ASTs
//! 3. **Collection** (`collection`) - Builds symbol tables from ASTs
//! 4. **Resolution** (`resolution`) - Resolves identifiers and types
//! 5. **Attribute Analysis** (`attribute_analysis`) - Performs semantic checks
//! 6. **Generation** (`generation`) - Emits bytecode class files
//!
//! ## Design Principles
//!
//! - **Separation of Concerns**: Each stage has a single responsibility
//! - **Testability**: Stages can be tested independently
//! - **Parallelism**: Stages support parallel processing where applicable
//! - **Error Handling**: Each stage provides detailed error information
//!
//! ## Stage Interfaces
//!
//! All stages follow a consistent pattern:
//! - Take well-defined inputs (e.g., file paths, ASTs)
//! - Process the data according to stage-specific logic
//! - Return structured results with comprehensive error handling
//! - Support both batch and incremental processing where applicable

/* 📖 # Why have a stages module?
The compilation process is naturally divided into distinct stages:
1. Discovery - finding Java source files
2. Parsing - converting source code to AST
3. Collection - building symbol tables
4. Resolution - resolving identifiers and types
5. Attribute analysis - semantic checks on resolved ASTs
6. Generation - emitting bytecode

Separating these into modules makes the code more organized,
easier to test individual stages, and clearer to understand
the compilation pipeline flow.
*/

pub mod attribute_analysis;
pub mod collection;
pub mod discovery;
pub mod generation;
pub mod parsing;
pub mod resolution;
