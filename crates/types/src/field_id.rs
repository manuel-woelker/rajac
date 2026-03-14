use std::fmt;

/// Identifier for a field signature stored in a `FieldArena`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(pub u32);

impl FieldId {
    /// Sentinel for an invalid or missing field id.
    pub const INVALID: FieldId = FieldId(u32::MAX);
}

impl fmt::Display for FieldId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldId({})", self.0)
    }
}
