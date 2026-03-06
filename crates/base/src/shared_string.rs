//! Shared string wrapper type for efficient string handling.

use crate::result::FelicoResult;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

/// A wrapper around smol_str::SmolStr for efficient shared string storage.
///
/// This type provides copy-on-write semantics with cheap cloning,
/// making it ideal for storing strings that are shared across multiple
/// parts of the compiler without unnecessary allocations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedString(pub SmolStr);

impl SharedString {
    /// Creates a new SharedString from the given string.
    pub fn new(s: impl Into<String>) -> Self {
        Self(SmolStr::from(s.into()))
    }

    /// Creates a new empty SharedString.
    pub fn empty() -> Self {
        Self(SmolStr::from(""))
    }

    /// Clears the contents of the string.
    pub fn clear(&mut self) {
        self.0 = SmolStr::from("");
    }

    /// Appends a string slice to this string.
    pub fn push_str(&mut self, string: &str) {
        let mut new_string = self.0.to_string();
        new_string.push_str(string);
        self.0 = SmolStr::from(new_string);
    }

    /// Returns the underlying string as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the length of the string.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn from_utf8(ut8_bytes: &[u8]) -> FelicoResult<Self> {
        Ok(Self(SmolStr::new(str::from_utf8(ut8_bytes)?)))
    }
}

impl Default for SharedString {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<String> for SharedString {
    fn from(s: String) -> Self {
        Self(SmolStr::from(s))
    }
}

impl From<&str> for SharedString {
    fn from(s: &str) -> Self {
        Self(SmolStr::from(s))
    }
}

impl From<Box<str>> for SharedString {
    fn from(s: Box<str>) -> Self {
        Self(SmolStr::from(&*s))
    }
}

impl From<&SharedString> for SharedString {
    fn from(s: &SharedString) -> Self {
        s.clone()
    }
}

impl AsRef<str> for SharedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for SharedString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for SharedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<str> for SharedString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SharedString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<SharedString> for str {
    fn eq(&self, other: &SharedString) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<SharedString> for &str {
    fn eq(&self, other: &SharedString) -> bool {
        *self == other.as_str()
    }
}

impl<'a, C> speedy::Readable<'a, C> for SharedString
where
    C: speedy::Context,
{
    fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let string = String::read_from(reader)?;
        Ok(SharedString(SmolStr::from(string)))
    }
}

impl<C> speedy::Writable<C> for SharedString
where
    C: speedy::Context,
{
    fn write_to<W>(&self, writer: &mut W) -> Result<(), C::Error>
    where
        W: speedy::Writer<C> + ?Sized,
    {
        self.as_str().write_to(writer)
    }
}

impl Serialize for SharedString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SharedString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use speedy::{Readable, Writable};

    #[test]
    fn test_shared_string_creation() {
        let s1 = SharedString::new("hello");
        let s2 = SharedString::from("world");
        let s3: SharedString = "test".into();

        assert_eq!(s1.as_str(), "hello");
        assert_eq!(s2.as_str(), "world");
        assert_eq!(s3.as_str(), "test");
    }

    #[test]
    fn test_shared_string_equality() {
        let s1 = SharedString::new("hello");
        let s2 = SharedString::new("hello");
        let s3 = SharedString::new("world");

        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_shared_string_clone() {
        let s1 = SharedString::new("hello");
        let s2 = s1.clone();

        assert_eq!(s1, s2);
        assert_eq!(s1.as_str(), s2.as_str());
    }

    #[test]
    fn test_shared_string_default() {
        let s = SharedString::default();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn test_shared_string_deref() {
        let s = SharedString::new("hello");
        assert_eq!(s.len(), 5);
        assert_eq!(&s[0..2], "he");
    }

    #[test]
    fn test_shared_string_display() {
        let s = SharedString::new("hello");
        assert_eq!(format!("{}", s), "hello");
    }

    #[test]
    fn test_shared_string_speedy_serialization() {
        let original = SharedString::new("hello world");

        // Test writing
        let buffer = original.write_to_vec().unwrap();
        assert!(!buffer.is_empty());

        // Test reading
        let deserialized = SharedString::read_from_buffer(&buffer).unwrap();
        assert_eq!(original, deserialized);
        assert_eq!(deserialized.as_str(), "hello world");
    }

    #[test]
    fn test_shared_string_speedy_empty_string() {
        let original = SharedString::empty();

        let buffer = original.write_to_vec().unwrap();
        let deserialized = SharedString::read_from_buffer(&buffer).unwrap();

        assert_eq!(original, deserialized);
        assert!(deserialized.is_empty());
    }

    #[test]
    fn test_shared_string_speedy_unicode() {
        let original = SharedString::new("Hello 🌍 世界");

        let buffer = original.write_to_vec().unwrap();
        let deserialized = SharedString::read_from_buffer(&buffer).unwrap();

        assert_eq!(original, deserialized);
        assert_eq!(deserialized.as_str(), "Hello 🌍 世界");
    }
}
