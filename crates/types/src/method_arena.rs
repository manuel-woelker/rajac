use crate::{MethodId, MethodSignature};

/// Arena storage for method signatures.
#[derive(Debug, Clone)]
pub struct MethodArena {
    methods: Vec<MethodSignature>,
}

impl MethodArena {
    pub fn new() -> Self {
        Self {
            methods: Vec::new(),
        }
    }

    pub fn alloc(&mut self, method: MethodSignature) -> MethodId {
        let id = MethodId(self.methods.len() as u32);
        self.methods.push(method);
        id
    }

    pub fn get(&self, id: MethodId) -> &MethodSignature {
        &self.methods[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: MethodId) -> &mut MethodSignature {
        &mut self.methods[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.methods.len()
    }

    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }
}

impl Default for MethodArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MethodModifiers;
    use rajac_base::shared_string::SharedString;

    #[test]
    fn allocates_and_returns_method_signatures() {
        let mut arena = MethodArena::new();
        let signature = MethodSignature::new(
            SharedString::new("run"),
            Vec::new(),
            crate::TypeId(0),
            MethodModifiers(MethodModifiers::PUBLIC),
        );

        let id = arena.alloc(signature.clone());

        assert_eq!(arena.get(id), &signature);
        assert_eq!(arena.len(), 1);
    }
}
