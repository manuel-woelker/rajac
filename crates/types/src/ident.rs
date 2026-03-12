use crate::TypeId;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: SharedString,
}

impl Ident {
    pub fn new(name: SharedString) -> Self {
        Self { name }
    }

    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Ident,
    pub bounds: Vec<TypeId>,
}
