use crate::PackageTable;
use rajac_base::shared_string::SharedString;
use rajac_types::{MethodArena, PrimitiveType, Type, TypeArena, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SymbolTable {
    packages: HashMap<String, PackageTable>,
    pub(crate) type_arena: TypeArena,
    pub(crate) method_arena: MethodArena,
    primitive_types: HashMap<SharedString, TypeId>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut type_arena = TypeArena::new();
        let primitive_types = seed_primitive_types(&mut type_arena);
        Self {
            packages: HashMap::new(),
            type_arena,
            method_arena: MethodArena::new(),
            primitive_types,
        }
    }

    pub fn type_arena_mut(&mut self) -> &mut TypeArena {
        &mut self.type_arena
    }

    pub fn type_arena(&self) -> &TypeArena {
        &self.type_arena
    }

    pub fn method_arena_mut(&mut self) -> &mut MethodArena {
        &mut self.method_arena
    }

    pub fn method_arena(&self) -> &MethodArena {
        &self.method_arena
    }

    pub fn arenas_mut(&mut self) -> (&mut TypeArena, &mut MethodArena) {
        (&mut self.type_arena, &mut self.method_arena)
    }

    pub fn primitive_type_id(&self, name: &str) -> Option<TypeId> {
        self.primitive_types.get(&SharedString::new(name)).copied()
    }

    pub fn primitive_type_id_by_kind(&self, kind: PrimitiveType) -> Option<TypeId> {
        self.primitive_type_id(primitive_type_name(&kind))
    }

    pub fn primitive_types(&self) -> &HashMap<SharedString, TypeId> {
        &self.primitive_types
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

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

fn seed_primitive_types(type_arena: &mut TypeArena) -> HashMap<SharedString, TypeId> {
    let mut primitive_types = HashMap::new();
    for primitive in [
        PrimitiveType::Boolean,
        PrimitiveType::Byte,
        PrimitiveType::Char,
        PrimitiveType::Short,
        PrimitiveType::Int,
        PrimitiveType::Long,
        PrimitiveType::Float,
        PrimitiveType::Double,
        PrimitiveType::Void,
    ] {
        let type_id = type_arena.alloc(Type::primitive(primitive.clone()));
        primitive_types.insert(SharedString::new(primitive_type_name(&primitive)), type_id);
    }
    primitive_types
}

fn primitive_type_name(kind: &PrimitiveType) -> &'static str {
    match kind {
        PrimitiveType::Boolean => "boolean",
        PrimitiveType::Byte => "byte",
        PrimitiveType::Char => "char",
        PrimitiveType::Short => "short",
        PrimitiveType::Int => "int",
        PrimitiveType::Long => "long",
        PrimitiveType::Float => "float",
        PrimitiveType::Double => "double",
        PrimitiveType::Void => "void",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_primitive_type_ids() {
        let table = SymbolTable::new();
        let int_id = table.primitive_type_id("int").expect("int type id");
        assert_eq!(
            table.type_arena().get(int_id),
            &Type::primitive(PrimitiveType::Int)
        );
    }
}
