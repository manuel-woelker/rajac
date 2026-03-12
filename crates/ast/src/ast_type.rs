use rajac_base::shared_string::SharedString;
use rajac_types::TypeId;

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
        ty: TypeId,
    },
    Array {
        element_type: AstTypeId,
        dimensions: u32,
        ty: TypeId,
    },
    Primitive {
        kind: PrimitiveType,
        ty: TypeId,
    },
    Wildcard {
        bound: Option<WildcardBound>,
        ty: TypeId,
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
            ty: TypeId::INVALID,
        }
    }

    pub fn simple_with_args(name: SharedString, type_args: Vec<AstTypeId>) -> Self {
        Self::Simple {
            name,
            type_args,
            ty: TypeId::INVALID,
        }
    }

    pub fn array(element_type: AstTypeId, dimensions: u32) -> Self {
        Self::Array {
            element_type,
            dimensions,
            ty: TypeId::INVALID,
        }
    }

    pub fn primitive(kind: PrimitiveType) -> Self {
        Self::Primitive {
            kind,
            ty: TypeId::INVALID,
        }
    }

    pub fn wildcard(bound: Option<WildcardBound>) -> Self {
        Self::Wildcard {
            bound,
            ty: TypeId::INVALID,
        }
    }

    pub fn error() -> Self {
        Self::Error
    }

    pub fn ty(&self) -> TypeId {
        match self {
            AstType::Simple { ty, .. } => *ty,
            AstType::Array { ty, .. } => *ty,
            AstType::Primitive { ty, .. } => *ty,
            AstType::Wildcard { ty, .. } => *ty,
            AstType::Error => TypeId::INVALID,
        }
    }

    pub fn set_ty(&mut self, ty: TypeId) {
        match self {
            AstType::Simple { ty: field, .. } => *field = ty,
            AstType::Array { ty: field, .. } => *field = ty,
            AstType::Primitive { ty: field, .. } => *field = ty,
            AstType::Wildcard { ty: field, .. } => *field = ty,
            AstType::Error => {} // Can't set type for Error
        }
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
