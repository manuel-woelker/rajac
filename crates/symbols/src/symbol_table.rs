use crate::PackageTable;
use rajac_base::shared_string::SharedString;
use rajac_types::{Type, TypeArena, TypeId};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SymbolTable {
    packages: HashMap<String, PackageTable>,
    pub(crate) type_arena: TypeArena,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            type_arena: TypeArena::new(),
        }
    }

    pub fn type_arena_mut(&mut self) -> &mut TypeArena {
        &mut self.type_arena
    }

    pub fn type_arena(&self) -> &TypeArena {
        &self.type_arena
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

    pub fn add_class(
        &mut self,
        package_name: &str,
        class_name: &str,
        ty: Type,
        kind: crate::SymbolKind,
    ) -> TypeId {
        let type_id = self.type_arena.alloc(ty);
        let package = self.package(package_name);
        let name = SharedString::new(class_name);
        package.insert(name.clone(), crate::Symbol::new(name, kind, type_id));
        type_id
    }
}
