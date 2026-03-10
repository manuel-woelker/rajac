use crate::SymbolKind;
use rajac_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: SharedString,
    pub kind: SymbolKind,
}

impl Symbol {
    pub fn new(name: SharedString, kind: SymbolKind) -> Self {
        Self { name, kind }
    }

    pub fn class(name: SharedString) -> Self {
        Self {
            name,
            kind: SymbolKind::Class,
        }
    }

    pub fn interface(name: SharedString) -> Self {
        Self {
            name,
            kind: SymbolKind::Interface,
        }
    }
}
