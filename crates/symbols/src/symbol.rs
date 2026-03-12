use crate::SymbolKind;
use rajac_base::shared_string::SharedString;
use rajac_types::TypeId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: SharedString,
    pub kind: SymbolKind,
    pub ty: TypeId,
}

impl Symbol {
    pub fn new(name: SharedString, kind: SymbolKind, ty: TypeId) -> Self {
        Self { name, kind, ty }
    }

    pub fn class(name: SharedString, ty: TypeId) -> Self {
        Self {
            name,
            kind: SymbolKind::Class,
            ty,
        }
    }

    pub fn interface(name: SharedString, ty: TypeId) -> Self {
        Self {
            name,
            kind: SymbolKind::Interface,
            ty,
        }
    }
}
