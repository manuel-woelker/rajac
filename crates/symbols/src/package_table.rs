use crate::Symbol;
use rajac_base::shared_string::SharedString;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct PackageTable {
    symbols: HashMap<SharedString, Symbol>,
}

impl PackageTable {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn insert(&mut self, name: SharedString, symbol: Symbol) -> Option<Symbol> {
        self.symbols.insert(name, symbol)
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SharedString, &Symbol)> {
        self.symbols.iter()
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}
