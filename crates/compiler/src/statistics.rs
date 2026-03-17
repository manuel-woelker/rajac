//! # Compilation Statistics
//!
//! This module provides timing and statistics collection for the compiler pipeline.
//! It tracks the duration of each compilation phase and provides formatted output.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

/// Collector for compilation statistics.
#[derive(Debug, Clone, Default)]
pub struct CompilationStatistics {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    timings: HashMap<CompilationPhase, Duration>,
    active: HashMap<CompilationPhase, Instant>,
}

impl CompilationStatistics {
    /// Creates a new empty statistics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins timing a phase.
    pub fn begin_phase(&self, phase: CompilationPhase) {
        self.inner
            .lock()
            .unwrap()
            .active
            .insert(phase, Instant::now());
    }

    /// Ends timing for a phase.
    pub fn end_phase(&self, phase: CompilationPhase) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(start) = inner.active.remove(&phase) {
            inner.timings.insert(phase, start.elapsed());
        }
    }

    /// Returns the timing for a specific phase.
    pub fn timing(&self, phase: CompilationPhase) -> Option<Duration> {
        self.inner.lock().unwrap().timings.get(&phase).copied()
    }

    /// Prints the statistics as a formatted table.
    pub fn print_table(&self) {
        let timings = self.inner.lock().unwrap().timings.clone();
        if timings.is_empty() {
            return;
        }

        println!();
        println!("=== Compilation Statistics ===");
        println!();

        let phases = [
            CompilationPhase::Parse,
            CompilationPhase::ClasspathCollect,
            CompilationPhase::Collection,
            CompilationPhase::Resolution,
            CompilationPhase::AttributeAnalysis,
            CompilationPhase::FlowAnalysis,
            CompilationPhase::Generation,
        ];

        let name_width = phases
            .iter()
            .map(|p| p.name().len())
            .max()
            .unwrap_or(0)
            .max(20);

        println!(
            "{:<width$} {:>12} {:>12}",
            "Phase",
            "Duration",
            "Percentage",
            width = name_width
        );

        println!(
            "{:-<width$} {:-^12} {:-^12}",
            "",
            "",
            "",
            width = name_width
        );

        let total: Duration = timings.values().sum();

        for phase in phases {
            if let Some(duration) = timings.get(&phase) {
                let percentage = if total.as_nanos() > 0 {
                    (duration.as_nanos() as f64 / total.as_nanos() as f64) * 100.0
                } else {
                    0.0
                };

                println!(
                    "{:<width$} {:>12} {:>11.1}%",
                    phase.name(),
                    format_duration(*duration),
                    percentage,
                    width = name_width
                );
            }
        }

        println!(
            "{:-<width$} {:-^12} {:-^12}",
            "",
            "",
            "",
            width = name_width
        );

        println!(
            "{:<width$} {:>12}",
            "Total",
            format_duration(total),
            width = name_width
        );

        println!();
    }
}

fn format_duration(duration: Duration) -> String {
    let nanos = duration.as_nanos();

    if nanos < 1_000 {
        format!("{} ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2} µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

/// Represents different phases of compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilationPhase {
    Parse,
    ClasspathCollect,
    Collection,
    Resolution,
    AttributeAnalysis,
    FlowAnalysis,
    Generation,
}

impl CompilationPhase {
    /// Returns the human-readable name of the phase.
    pub fn name(self) -> &'static str {
        match self {
            CompilationPhase::Parse => "Parse",
            CompilationPhase::ClasspathCollect => "Classpath Collect",
            CompilationPhase::Collection => "Collection",
            CompilationPhase::Resolution => "Resolution",
            CompilationPhase::AttributeAnalysis => "Attribute Analysis",
            CompilationPhase::FlowAnalysis => "Flow Analysis",
            CompilationPhase::Generation => "Generation",
        }
    }
}
