use crate::{FieldId, FieldSignature};

/// Arena storage for field signatures.
#[derive(Debug, Clone)]
pub struct FieldArena {
    fields: Vec<FieldSignature>,
}

impl FieldArena {
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    pub fn alloc(&mut self, field: FieldSignature) -> FieldId {
        let id = FieldId(self.fields.len() as u32);
        self.fields.push(field);
        id
    }

    pub fn get(&self, id: FieldId) -> &FieldSignature {
        &self.fields[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: FieldId) -> &mut FieldSignature {
        &mut self.fields[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl Default for FieldArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldModifiers;
    use rajac_base::shared_string::SharedString;

    #[test]
    fn allocates_and_returns_field_signatures() {
        let mut arena = FieldArena::new();
        let signature = FieldSignature::new(
            SharedString::new("count"),
            crate::TypeId(0),
            FieldModifiers(FieldModifiers::PUBLIC),
        );

        let id = arena.alloc(signature.clone());

        assert_eq!(arena.get(id), &signature);
        assert_eq!(arena.len(), 1);
    }
}
