//! Qualified name representation for name resolution.

use crate::shared_string::SharedString;
use serde::{Deserialize, Serialize};
use speedy::{Readable, Writable};

/// # What does this represent?
/// A package-qualified name represented by a package and a local identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Readable, Writable, Serialize, Deserialize)]
pub struct QualifiedName {
    /// The package portion (empty for the default package).
    package_name: SharedString,
    /// The local identifier within the package.
    name: SharedString,
}

impl QualifiedName {
    /// Creates a new qualified name from a package and local name.
    pub fn new(package_name: SharedString, name: SharedString) -> Self {
        Self { package_name, name }
    }

    /// Creates a qualified name in the default package.
    pub fn from_ident(name: SharedString) -> Self {
        Self {
            package_name: SharedString::empty(),
            name,
        }
    }

    /// Returns the package portion (empty for the default package).
    pub fn package_name(&self) -> &SharedString {
        &self.package_name
    }

    /// Returns the local identifier portion.
    pub fn name(&self) -> &SharedString {
        &self.name
    }

    /// Returns true if this name is in the default package.
    pub fn is_default_package(&self) -> bool {
        self.package_name.is_empty()
    }
}

impl std::fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.package_name.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}.{}", self.package_name, self.name)
        }
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
    fn from_ident_creates_default_package() {
        let name = QualifiedName::from_ident("foo".into());
        assert!(name.is_default_package());
        assert_eq!(name.name().as_str(), "foo");
    }

    #[test]
    fn display_joins_package_and_name() {
        let name = QualifiedName::new("java.util".into(), "HashMap".into());
        assert_eq!(name.to_string(), "java.util.HashMap");
    }

    #[test]
    fn display_default_package_is_name_only() {
        let name = QualifiedName::new(SharedString::empty(), "Bar".into());
        assert_eq!(name.to_string(), "Bar");
    }

    #[test]
    fn from_shared_string_uses_default_package() {
        let name: QualifiedName = "foo".into();
        assert!(name.is_default_package());
        assert_eq!(name.name().as_str(), "foo");
    }
}
