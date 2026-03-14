use crate::TypeId;
use rajac_base::shared_string::SharedString;

/// Method modifiers used during type checking and overload resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodModifiers(pub u32);

impl MethodModifiers {
    /// Method is visible to all.
    pub const PUBLIC: u32 = 0x0001;
    /// Method is only visible within the declaring class.
    pub const PRIVATE: u32 = 0x0002;
    /// Method is visible to subclasses and same-package types.
    pub const PROTECTED: u32 = 0x0004;
    /// Method is declared as static.
    pub const STATIC: u32 = 0x0008;
    /// Method is declared as final.
    pub const FINAL: u32 = 0x0010;
    /// Method is declared as abstract.
    pub const ABSTRACT: u32 = 0x0400;
    /// Method is declared as native.
    pub const NATIVE: u32 = 0x0100;
    /// Method is declared as synchronized.
    pub const SYNCHRONIZED: u32 = 0x0020;
    /// Method is declared as strictfp.
    pub const STRICTFP: u32 = 0x0800;

    /// Returns true when the method is marked as `static`.
    pub fn is_static(&self) -> bool {
        self.0 & Self::STATIC != 0
    }
}

/// Type-level method signature for overload resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSignature {
    /// Method name (constructors use the class name).
    pub name: SharedString,
    /// Parameter types in declaration order.
    pub params: Vec<TypeId>,
    /// Return type for the method.
    pub return_type: TypeId,
    /// Declared checked exceptions.
    pub throws: Vec<TypeId>,
    /// Visibility and behavior modifiers.
    pub modifiers: MethodModifiers,
}

impl MethodSignature {
    pub fn new(
        name: SharedString,
        params: Vec<TypeId>,
        return_type: TypeId,
        modifiers: MethodModifiers,
    ) -> Self {
        Self {
            name,
            params,
            return_type,
            throws: Vec::new(),
            modifiers,
        }
    }

    pub fn with_throws(mut self, throws: Vec<TypeId>) -> Self {
        self.throws = throws;
        self
    }
}
