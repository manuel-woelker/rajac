use crate::TypeId;
use rajac_base::shared_string::SharedString;
use std::fmt::{Display, Formatter};

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

impl Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Ident,
    pub bounds: Vec<TypeId>,
}
