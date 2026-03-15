//! # Compilation Result
//!
//! This module defines the result returned by the compiler after a compilation
//! run completes.

use crate::statistics::CompilationStatistics;
use rajac_diagnostics::{Diagnostics, Severity};

/// Result of a completed compilation run.
///
/// This contains the diagnostics emitted during compilation together with the
/// timing statistics collected for the executed phases.
#[derive(Debug)]
pub struct CompilationResult {
    /// Diagnostics emitted during compilation.
    pub diagnostics: Diagnostics,
    /// Timing statistics collected during compilation.
    pub statistics: CompilationStatistics,
}

impl CompilationResult {
    /// Creates a new compilation result.
    pub fn new(diagnostics: Diagnostics, statistics: CompilationStatistics) -> Self {
        Self {
            diagnostics,
            statistics,
        }
    }

    /// Returns whether any error diagnostics were emitted.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}
