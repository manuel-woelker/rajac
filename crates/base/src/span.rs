use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Shared storage for span byte offsets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanBase {
    range: Range<usize>,
}

impl SpanBase {
    /// Creates a span base from start and end byte offsets.
    pub fn new(start: usize, end: usize) -> Self {
        Self { range: start..end }
    }

    /// Returns the start byte offset (inclusive).
    pub fn start(&self) -> usize {
        self.range.start
    }

    /// Returns the end byte offset (exclusive).
    pub fn end(&self) -> usize {
        self.range.end
    }

    /// Returns the wrapped range.
    pub fn range(&self) -> &Range<usize> {
        &self.range
    }
}

/// Byte span within a source text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    base: SpanBase,
}

impl Span {
    /// Creates a span from start and end byte offsets.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            base: SpanBase::new(start, end),
        }
    }

    /// Returns the start byte offset (inclusive).
    pub fn start(&self) -> usize {
        self.base.start()
    }

    /// Returns the end byte offset (exclusive).
    pub fn end(&self) -> usize {
        self.base.end()
    }

    /// Returns the wrapped range.
    pub fn range(&self) -> &Range<usize> {
        self.base.range()
    }
}

/// Byte span relative to a local base offset.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelativeSpan {
    base: SpanBase,
}

impl RelativeSpan {
    /// Creates a relative span from start and end byte offsets.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            base: SpanBase::new(start, end),
        }
    }

    /// Returns the start byte offset (inclusive).
    pub fn start(&self) -> usize {
        self.base.start()
    }

    /// Returns the end byte offset (exclusive).
    pub fn end(&self) -> usize {
        self.base.end()
    }

    /// Returns the wrapped range.
    pub fn range(&self) -> &Range<usize> {
        self.base.range()
    }
}
