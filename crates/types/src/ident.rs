use crate::TypeId;
use rajac_base::qualified_name::QualifiedName as ResolvedName;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: SharedString,
    pub qualified_name: ResolvedName,
}

impl Ident {
    pub fn new(name: SharedString) -> Self {
        Self {
            name,
            qualified_name: ResolvedName::default(),
        }
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
