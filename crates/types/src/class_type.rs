use crate::TypeId;

#[derive(Debug, Clone, PartialEq)]
pub struct ClassType {
    pub name: String,
    pub package: Option<String>,
    pub type_args: Vec<TypeId>,
    pub superclass: Option<TypeId>,
    pub interfaces: Vec<TypeId>,
}

impl ClassType {
    pub fn new(name: String) -> Self {
        Self {
            name,
            package: None,
            type_args: Vec::new(),
            superclass: None,
            interfaces: Vec::new(),
        }
    }

    pub fn with_package(mut self, package_: String) -> Self {
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

    pub fn internal_name(&self) -> String {
        match &self.package {
            Some(pkg) => format!("{}/{}", pkg.replace('.', "/"), self.name),
            None => self.name.replace('.', "/"),
        }
    }
}
