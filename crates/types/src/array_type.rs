use crate::TypeId;

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayType {
    pub element_type: TypeId,
}

impl ArrayType {
    pub fn new(element_type: TypeId) -> Self {
        Self { element_type }
    }
}
