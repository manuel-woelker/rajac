use crate::bytecode::UnsupportedFeature;
use rajac_ast::{ClassDeclId, ClassKind, Modifiers};
use rajac_base::shared_string::SharedString;
use rajac_symbols::SymbolTable;
use ristretto_classfile::ClassFile;

#[derive(Clone, Debug)]
pub(crate) struct NestedClassInfo {
    pub(crate) class_id: ClassDeclId,
    pub(crate) internal_name: SharedString,
    pub(crate) simple_name: SharedString,
    pub(crate) modifiers: Modifiers,
    pub(crate) kind: ClassKind,
}

pub(crate) struct ClassfileGenerationContext<'a> {
    pub(crate) type_arena: &'a rajac_types::TypeArena,
    pub(crate) symbol_table: &'a SymbolTable,
    pub(crate) unsupported_features: &'a mut Vec<UnsupportedFeature>,
}

pub struct GeneratedClassFiles {
    pub class_files: Vec<ClassFile>,
    pub unsupported_features: Vec<UnsupportedFeature>,
}
