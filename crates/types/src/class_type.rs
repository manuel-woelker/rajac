use crate::{FieldId, MethodId, TypeId};
use rajac_base::shared_string::SharedString;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ClassType {
    /// Simple class name (without package).
    pub name: SharedString,
    /// Package name or `None` for the default package.
    pub package: Option<SharedString>,
    /// Type arguments applied to this class type.
    pub type_args: Vec<TypeId>,
    /// Superclass type id, if any.
    pub superclass: Option<TypeId>,
    /// Implemented interface type ids.
    pub interfaces: Vec<TypeId>,
    /// Method ids grouped by name for overload resolution.
    pub methods: HashMap<SharedString, Vec<MethodId>>,
    /// Field ids grouped by name.
    pub fields: HashMap<SharedString, Vec<FieldId>>,
}

impl ClassType {
    pub fn new(name: SharedString) -> Self {
        Self {
            name,
            package: None,
            type_args: Vec::new(),
            superclass: None,
            interfaces: Vec::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
        }
    }

    pub fn with_package(mut self, package_: SharedString) -> Self {
        self.package = Some(package_);
        self
    }

    pub fn with_type_args(mut self, type_args: Vec<TypeId>) -> Self {
        self.type_args = type_args;
        self
    }

    pub fn with_superclass(mut self, superclass: TypeId) -> Self {
        self.superclass = Some(superclass);
        self
    }

    pub fn with_interfaces(mut self, interfaces: Vec<TypeId>) -> Self {
        self.interfaces = interfaces;
        self
    }

    pub fn with_methods(mut self, methods: HashMap<SharedString, Vec<MethodId>>) -> Self {
        self.methods = methods;
        self
    }

    pub fn with_fields(mut self, fields: HashMap<SharedString, Vec<FieldId>>) -> Self {
        self.fields = fields;
        self
    }

    pub fn add_method(&mut self, name: SharedString, method_id: MethodId) {
        self.methods.entry(name).or_default().push(method_id);
    }

    pub fn add_field(&mut self, name: SharedString, field_id: FieldId) {
        self.fields.entry(name).or_default().push(field_id);
    }

    pub fn internal_name(&self) -> String {
        match &self.package {
            Some(pkg) => format!("{}/{}", pkg.as_str().replace('.', "/"), self.name.as_str()),
            None => self.name.as_str().replace('.', "/"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldId, MethodId};

    #[test]
    fn add_method_groups_by_name() {
        let mut class_type = ClassType::new(SharedString::new("Widget"));
        class_type.add_method(SharedString::new("size"), MethodId(0));
        class_type.add_method(SharedString::new("size"), MethodId(1));

        let methods = class_type.methods.get("size").expect("methods");
        assert_eq!(methods, &[MethodId(0), MethodId(1)]);
    }

    #[test]
    fn add_field_groups_by_name() {
        let mut class_type = ClassType::new(SharedString::new("Widget"));
        class_type.add_field(SharedString::new("name"), FieldId(0));
        class_type.add_field(SharedString::new("name"), FieldId(1));

        let fields = class_type.fields.get("name").expect("fields");
        assert_eq!(fields, &[FieldId(0), FieldId(1)]);
    }
}
