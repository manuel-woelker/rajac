use crate::PackageTable;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SymbolTable {
    packages: HashMap<String, PackageTable>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn package(&mut self, name: &str) -> &mut PackageTable {
        self.packages.entry(name.to_string()).or_default()
    }

    pub fn get_package(&self, name: &str) -> Option<&PackageTable> {
        self.packages.get(name)
    }

    pub fn contains_package(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &PackageTable)> {
        self.packages.iter()
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}
