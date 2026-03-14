use std::fmt;

/// Identifier for a method signature stored in a `MethodArena`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodId(pub u32);

impl MethodId {
    /// Sentinel for an invalid or missing method id.
    pub const INVALID: MethodId = MethodId(u32::MAX);
}

impl fmt::Display for MethodId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MethodId({})", self.0)
    }
}
