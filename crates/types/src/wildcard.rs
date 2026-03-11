use crate::TypeId;

#[derive(Debug, Clone, PartialEq)]
pub struct WildcardType {
    pub bound: Option<WildcardBound>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WildcardBound {
    Extends(TypeId),
    Super(TypeId),
}
