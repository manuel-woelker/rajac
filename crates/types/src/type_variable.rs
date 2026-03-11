use crate::TypeId;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeVariable {
    pub name: String,
    pub bound: Option<TypeId>,
}

impl TypeVariable {
    pub fn new(name: String) -> Self {
        Self { name, bound: None }
    }

    pub fn with_bound(mut self, bound: TypeId) -> Self {
        self.bound = Some(bound);
        self
    }
}
