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
        // Special case for common Java types that should be fully qualified
        if self.package.is_none() {
            match self.name.as_str() {
                "String" => return "java/lang/String".to_string(),
                "Object" => return "java/lang/Object".to_string(),
                "System" => return "java/lang/System".to_string(),
                "PrintStream" => return "java/io/PrintStream".to_string(),
                _ => {}
            }
        }

        match &self.package {
            Some(pkg) => format!("{}/{}", pkg.replace('.', "/"), self.name),
            None => self.name.replace('.', "/"),
        }
    }
}
