use crate::{
    ArrayType, ClassType, PrimitiveType, TypeId, TypeVariable, WildcardBound, WildcardType,
};
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Primitive(PrimitiveType),
    Class(ClassType),
    Array(ArrayType),
    TypeVariable(TypeVariable),
    Wildcard(WildcardType),
    Error,
}

impl Type {
    pub fn primitive(primitive: PrimitiveType) -> Self {
        Type::Primitive(primitive)
    }

    pub fn class(class: ClassType) -> Self {
        Type::Class(class)
    }

    pub fn array(element_type: TypeId) -> Self {
        Type::Array(ArrayType::new(element_type))
    }

    pub fn type_variable(name: SharedString) -> Self {
        Type::TypeVariable(TypeVariable::new(name))
    }

    pub fn wildcard(bound: Option<WildcardBound>) -> Self {
        Type::Wildcard(WildcardType { bound })
    }

    pub fn error() -> Self {
        Type::Error
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, Type::Primitive(_))
    }

    pub fn is_class(&self) -> bool {
        matches!(self, Type::Class(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Type::Array(_))
    }

    pub fn is_type_variable(&self) -> bool {
        matches!(self, Type::TypeVariable(_))
    }

    pub fn is_wildcard(&self) -> bool {
        matches!(self, Type::Wildcard(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Type::Error)
    }
}
