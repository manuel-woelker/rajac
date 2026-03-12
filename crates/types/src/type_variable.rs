use crate::TypeId;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeVariable {
    pub name: SharedString,
    pub bound: Option<TypeId>,
}

impl TypeVariable {
    pub fn new(name: SharedString) -> Self {
        Self { name, bound: None }
    }

    pub fn with_bound(mut self, bound: TypeId) -> Self {
        self.bound = Some(bound);
        self
    }
}
