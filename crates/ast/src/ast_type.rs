use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq)]
pub struct AstTypeParam {
    pub name: SharedString,
    pub bounds: Vec<AstTypeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstType {
    Simple {
        name: SharedString,
        type_args: Vec<AstTypeId>,
    },
    Array {
        element_type: AstTypeId,
        dimensions: u32,
    },
    Primitive {
        kind: PrimitiveType,
    },
    Wildcard {
        bound: Option<WildcardBound>,
    },
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveType {
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    Char,
    Boolean,
    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WildcardBound {
    Extends(AstTypeId),
    Super(AstTypeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AstTypeId(pub u32);

impl AstType {
    pub fn simple(name: SharedString) -> Self {
        Self::Simple {
            name,
            type_args: Vec::new(),
        }
    }

    pub fn simple_with_args(name: SharedString, type_args: Vec<AstTypeId>) -> Self {
        Self::Simple { name, type_args }
    }

    pub fn array(element_type: AstTypeId, dimensions: u32) -> Self {
        Self::Array {
            element_type,
            dimensions,
        }
    }

    pub fn primitive(kind: PrimitiveType) -> Self {
        Self::Primitive { kind }
    }

    pub fn wildcard(bound: Option<WildcardBound>) -> Self {
        Self::Wildcard { bound }
    }

    pub fn error() -> Self {
        Self::Error
    }

    pub fn is_simple(&self) -> bool {
        matches!(self, AstType::Simple { .. })
    }

    pub fn is_array(&self) -> bool {
        matches!(self, AstType::Array { .. })
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, AstType::Primitive { .. })
    }

    pub fn is_wildcard(&self) -> bool {
        matches!(self, AstType::Wildcard { .. })
    }

    pub fn is_error(&self) -> bool {
        matches!(self, AstType::Error)
    }
}

impl AstTypeId {
    pub const INVALID: AstTypeId = AstTypeId(u32::MAX);
}
