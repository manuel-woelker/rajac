use crate::{Type, TypeId};

#[derive(Debug)]
pub struct TypeArena {
    types: Vec<Type>,
}

impl TypeArena {
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }

    pub fn alloc(&mut self, ty: Type) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty);
        id
    }

    pub fn get(&self, id: TypeId) -> &Type {
        &self.types[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: TypeId) -> &mut Type {
        &mut self.types[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

impl Default for TypeArena {
    fn default() -> Self {
        Self::new()
    }
}
