//! Qualified name representation for name resolution.

use crate::shared_string::SharedString;
use serde::{Deserialize, Serialize};
use speedy::{Readable, Writable};

/// A fully qualified name for an item, represented as a sequence of namespace/module
/// segments followed by the final identifier.
///
/// For example, `std::collections::HashMap` would be represented as:
/// `QualifiedName { segments: ["std", "collections", "HashMap"] }`
///
/// This structure preserves the hierarchical nature of qualified names, which is
/// essential for name resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Readable, Writable, Serialize, Deserialize)]
pub struct QualifiedName {
    /// The sequence of name segments (e.g., `["std", "collections", "HashMap"]`).
    segments: Vec<SharedString>,
}

impl QualifiedName {
    /// Creates a new qualified name from segments.
    pub fn new(segments: Vec<SharedString>) -> Self {
        Self { segments }
    }

    /// Creates a qualified name from a single identifier.
    pub fn from_ident(name: SharedString) -> Self {
        Self {
            segments: vec![name],
        }
    }

    /// Returns the final identifier (the last segment).
    ///
    /// # Panics
    /// Panics if the qualified name has no segments.
    pub fn ident(&self) -> &SharedString {
        self.segments
            .last()
            .expect("qualified name has at least one segment")
    }

    /// Returns the namespace/module path (all segments except the last).
    ///
    /// Returns an empty slice for simple identifiers.
    pub fn namespace(&self) -> &[SharedString] {
        &self.segments[..self.segments.len().saturating_sub(1)]
    }

    /// Returns the number of segments in this qualified name.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Returns true if this qualified name has no segments.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns an iterator over the segments.
    pub fn segments(&self) -> &[SharedString] {
        &self.segments
    }

    /// Joins a new segment to create a new QualifiedName
    pub fn join(&self, segment: impl Into<SharedString>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment.into());
        Self { segments }
    }
}

impl std::fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.segments.join("::"))
    }
}

impl From<SharedString> for QualifiedName {
    fn from(s: SharedString) -> Self {
        Self::from_ident(s)
    }
}

impl From<&str> for QualifiedName {
    fn from(s: &str) -> Self {
        Self::from_ident(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ident_creates_single_segment() {
        let name = QualifiedName::from_ident("foo".into());
        assert_eq!(name.len(), 1);
        assert_eq!(name.ident().as_str(), "foo");
    }

    #[test]
    fn namespace_returns_all_but_last() {
        let name = QualifiedName::new(vec!["std".into(), "collections".into(), "HashMap".into()]);
        let ns = name.namespace();
        assert_eq!(ns.len(), 2);
        assert_eq!(ns[0].as_str(), "std");
        assert_eq!(ns[1].as_str(), "collections");
    }

    #[test]
    fn to_string_joins_with_double_colon() {
        let name = QualifiedName::new(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(name.to_string(), "a::b::c");
    }

    #[test]
    fn display_formats_correctly() {
        let name = QualifiedName::new(vec!["foo".into(), "bar".into()]);
        assert_eq!(format!("{}", name), "foo::bar");
    }

    #[test]
    fn from_shared_string() {
        let name: QualifiedName = "foo".into();
        assert_eq!(name.len(), 1);
    }
}
