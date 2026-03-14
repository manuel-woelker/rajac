use crate::TypeId;
use rajac_base::shared_string::SharedString;

/// Field modifiers used during type checking and resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldModifiers(pub u32);

impl FieldModifiers {
    /// Field is visible to all.
    pub const PUBLIC: u32 = 0x0001;
    /// Field is only visible within the declaring class.
    pub const PRIVATE: u32 = 0x0002;
    /// Field is visible to subclasses and same-package types.
    pub const PROTECTED: u32 = 0x0004;
    /// Field is declared as static.
    pub const STATIC: u32 = 0x0008;
    /// Field is declared as final.
    pub const FINAL: u32 = 0x0010;
    /// Field is declared as volatile.
    pub const VOLATILE: u32 = 0x0040;
    /// Field is declared as transient.
    pub const TRANSIENT: u32 = 0x0080;
}

/// Type-level field signature for resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSignature {
    /// Field name.
    pub name: SharedString,
    /// Field type.
    pub ty: TypeId,
    /// Visibility and behavior modifiers.
    pub modifiers: FieldModifiers,
}

impl FieldSignature {
    pub fn new(name: SharedString, ty: TypeId, modifiers: FieldModifiers) -> Self {
        Self {
            name,
            ty,
            modifiers,
        }
    }
}
