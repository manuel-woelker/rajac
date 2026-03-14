use rajac_base::shared_string::SharedString;
use rajac_symbols::{Symbol, SymbolKind, SymbolTable};
use rayon::prelude::*;
use ristretto_classfile::{BaseType, ClassFile, FieldAccessFlags, FieldType, MethodAccessFlags};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::Instant;
use zip::ZipArchive;

pub struct Classpath {
    entries: Vec<ClasspathEntry>,
}

enum ClasspathEntry {
    Directory(PathBuf),
    Jar(PathBuf),
}

struct ParsedClass {
    package: SharedString,
    class_name: SharedString,
    is_interface: bool,
    super_class: Option<SharedString>,
    interfaces: Vec<SharedString>,
    methods: Vec<ParsedMethod>,
    fields: Vec<ParsedField>,
}

#[derive(Clone, Debug)]
struct ParsedMethod {
    name: SharedString,
    params: Vec<FieldType>,
    return_type: Option<FieldType>,
    modifiers: rajac_types::MethodModifiers,
}

#[derive(Clone, Debug)]
struct ParsedField {
    name: SharedString,
    ty: FieldType,
    modifiers: rajac_types::FieldModifiers,
}

impl Classpath {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add_directory(&mut self, path: impl Into<PathBuf>) {
        self.entries.push(ClasspathEntry::Directory(path.into()));
    }

    pub fn add_jar(&mut self, path: impl Into<PathBuf>) {
        self.entries.push(ClasspathEntry::Jar(path.into()));
    }

    pub fn add_to_symbol_table(&self, symbol_table: &mut SymbolTable) -> RajacResult<()> {
        for entry in &self.entries {
            match entry {
                ClasspathEntry::Directory(dir) => {
                    self.add_directory_to_symbol_table(dir, symbol_table)?;
                }
                ClasspathEntry::Jar(jar) => {
                    self.add_jar_to_symbol_table(jar, symbol_table)?;
                }
            }
        }
        Ok(())
    }

    fn add_directory_to_symbol_table(
        &self,
        dir: &Path,
        symbol_table: &mut SymbolTable,
    ) -> RajacResult<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        // First pass: Collect and parse all class files
        let mut parsed_classes = Vec::new();
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "class") {
                let bytes = std::fs::read(path).context("Failed to read class file")?;
                if let Ok(class_file) = ClassFile::from_bytes(&mut Cursor::new(bytes))
                    && let Some(parsed) = parse_class_file(&class_file)
                {
                    parsed_classes.push(parsed);
                }
            }
        }

        // Second pass: Collect raw data (without holding references to symbol_table)
        let class_info: Vec<_> = parsed_classes
            .iter()
            .map(|parsed_class| {
                let package_name = parsed_class.package.clone();
                let class_name = parsed_class.class_name.clone();
                let kind = if parsed_class.is_interface {
                    SymbolKind::Interface
                } else {
                    SymbolKind::Class
                };
                (package_name, class_name, kind)
            })
            .collect();

        // Third pass: Allocate types first
        let type_arena = symbol_table.type_arena_mut();
        let type_ids: Vec<_> = class_info
            .iter()
            .map(|(package_name, class_name, _)| {
                let class_type = if !package_name.is_empty() {
                    rajac_types::ClassType::new(class_name.clone())
                        .with_package(package_name.clone())
                } else {
                    rajac_types::ClassType::new(class_name.clone())
                };
                type_arena.alloc(rajac_types::Type::class(class_type))
            })
            .collect();

        // Fourth pass: Insert into symbol table
        for (type_id, (package_name, class_name, kind)) in type_ids.into_iter().zip(class_info) {
            let package = symbol_table.package(&package_name);
            package.insert(class_name.clone(), Symbol::new(class_name, kind, type_id));
        }

        // Fifth pass: Resolve superclass and interface relationships
        resolve_class_relationships(&parsed_classes, symbol_table)?;

        Ok(())
    }

    fn add_jar_to_symbol_table(
        &self,
        jar: &Path,
        symbol_table: &mut SymbolTable,
    ) -> RajacResult<()> {
        let file = File::open(jar).context("Failed to open JAR file")?;
        let mut archive = ZipArchive::new(file).context("Failed to read JAR file")?;

        let class_data: Vec<Vec<u8>> = (0..archive.len())
            .filter_map(|i| {
                let mut file = archive.by_index(i).ok()?;
                let name = file.name();
                if name.ends_with(".class") && !name.contains('$') {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes).ok()?;
                    Some(bytes)
                } else {
                    None
                }
            })
            .collect();

        drop(archive);

        let start = Instant::now();
        let parsed_classes: Vec<ParsedClass> = class_data
            .into_par_iter()
            .filter_map(|bytes| {
                let class_file = ClassFile::from_bytes(&mut Cursor::new(bytes)).ok()?;
                parse_class_file(&class_file)
            })
            .collect();
        println!(
            "Read {:?} in {}ms ({} classes)",
            jar,
            start.elapsed().as_millis(),
            parsed_classes.len()
        );

        // First pass: Collect raw data
        let class_info: Vec<_> = parsed_classes
            .iter()
            .map(|parsed_class| {
                let package_name = parsed_class.package.clone();
                let class_name = parsed_class.class_name.clone();
                let kind = if parsed_class.is_interface {
                    SymbolKind::Interface
                } else {
                    SymbolKind::Class
                };
                (package_name, class_name, kind)
            })
            .collect();

        // Second pass: Allocate types first
        let type_arena = symbol_table.type_arena_mut();
        let type_ids: Vec<_> = class_info
            .iter()
            .map(|(package_name, class_name, _)| {
                let class_type = if !package_name.is_empty() {
                    rajac_types::ClassType::new(class_name.clone())
                        .with_package(package_name.clone())
                } else {
                    rajac_types::ClassType::new(class_name.clone())
                };
                type_arena.alloc(rajac_types::Type::class(class_type))
            })
            .collect();

        // Third pass: Insert into symbol table
        for (type_id, (package_name, class_name, kind)) in type_ids.into_iter().zip(class_info) {
            let package = symbol_table.package(&package_name);
            package.insert(class_name.clone(), Symbol::new(class_name, kind, type_id));
        }

        // Fourth pass: Resolve superclass and interface relationships
        resolve_class_relationships(&parsed_classes, symbol_table)?;

        Ok(())
    }
}

impl Default for Classpath {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_class_file(class_file: &ClassFile) -> Option<ParsedClass> {
    let internal_name = class_file.class_name().ok()?;

    let (package, class_name) = if let Some(last_slash) = internal_name.rfind('/') {
        (
            SharedString::new(internal_name[..last_slash].replace('/', ".")),
            SharedString::new(&internal_name[last_slash + 1..]),
        )
    } else {
        (SharedString::new(""), SharedString::new(internal_name))
    };

    let is_interface = class_file
        .access_flags
        .contains(ristretto_classfile::ClassAccessFlags::INTERFACE);

    // Extract superclass information
    let super_class = if class_file.super_class != 0 {
        class_file
            .constant_pool
            .try_get_class(class_file.super_class)
            .ok()
            .map(|name| SharedString::new(name.replace('/', ".")))
    } else {
        None
    };

    // Extract interface information
    let interfaces: Vec<SharedString> = class_file
        .interfaces
        .iter()
        .filter_map(|&interface_idx| {
            class_file
                .constant_pool
                .try_get_class(interface_idx)
                .ok()
                .map(|name| SharedString::new(name.replace('/', ".")))
        })
        .collect();

    let methods: Vec<ParsedMethod> = class_file
        .methods
        .iter()
        .filter_map(|method| parse_class_method(class_file, method, &class_name))
        .collect();

    let fields: Vec<ParsedField> = class_file
        .fields
        .iter()
        .filter_map(|field| parse_class_field(class_file, field))
        .collect();

    Some(ParsedClass {
        package,
        class_name,
        is_interface,
        super_class,
        interfaces,
        methods,
        fields,
    })
}

fn parse_class_method(
    class_file: &ClassFile,
    method: &ristretto_classfile::Method,
    class_name: &SharedString,
) -> Option<ParsedMethod> {
    let raw_name = class_file
        .constant_pool
        .try_get_utf8(method.name_index)
        .ok()?;
    let name = method_name_for_class(raw_name, class_name)?;
    let descriptor = class_file
        .constant_pool
        .try_get_utf8(method.descriptor_index)
        .ok()?;
    let (params, return_type) = FieldType::parse_method_descriptor(descriptor).ok()?;

    Some(ParsedMethod {
        name,
        params,
        return_type,
        modifiers: method_modifiers_from_access_flags(method.access_flags),
    })
}

fn parse_class_field(
    class_file: &ClassFile,
    field: &ristretto_classfile::Field,
) -> Option<ParsedField> {
    let raw_name = class_file
        .constant_pool
        .try_get_utf8(field.name_index)
        .ok()?;
    let name = SharedString::new(raw_name);

    Some(ParsedField {
        name,
        ty: field.field_type.clone(),
        modifiers: field_modifiers_from_access_flags(field.access_flags),
    })
}

fn method_name_for_class(raw_name: &str, class_name: &SharedString) -> Option<SharedString> {
    match raw_name {
        "<clinit>" => None,
        "<init>" => Some(class_name.clone()),
        _ => Some(SharedString::new(raw_name)),
    }
}

fn method_modifiers_from_access_flags(
    access_flags: MethodAccessFlags,
) -> rajac_types::MethodModifiers {
    let mut bits = 0;
    if access_flags.contains(MethodAccessFlags::PUBLIC) {
        bits |= rajac_types::MethodModifiers::PUBLIC;
    }
    if access_flags.contains(MethodAccessFlags::PRIVATE) {
        bits |= rajac_types::MethodModifiers::PRIVATE;
    }
    if access_flags.contains(MethodAccessFlags::PROTECTED) {
        bits |= rajac_types::MethodModifiers::PROTECTED;
    }
    if access_flags.contains(MethodAccessFlags::STATIC) {
        bits |= rajac_types::MethodModifiers::STATIC;
    }
    if access_flags.contains(MethodAccessFlags::FINAL) {
        bits |= rajac_types::MethodModifiers::FINAL;
    }
    if access_flags.contains(MethodAccessFlags::ABSTRACT) {
        bits |= rajac_types::MethodModifiers::ABSTRACT;
    }
    if access_flags.contains(MethodAccessFlags::NATIVE) {
        bits |= rajac_types::MethodModifiers::NATIVE;
    }
    if access_flags.contains(MethodAccessFlags::SYNCHRONIZED) {
        bits |= rajac_types::MethodModifiers::SYNCHRONIZED;
    }
    if access_flags.contains(MethodAccessFlags::STRICT) {
        bits |= rajac_types::MethodModifiers::STRICTFP;
    }

    rajac_types::MethodModifiers(bits)
}

fn field_modifiers_from_access_flags(
    access_flags: FieldAccessFlags,
) -> rajac_types::FieldModifiers {
    let mut bits = 0;
    if access_flags.contains(FieldAccessFlags::PUBLIC) {
        bits |= rajac_types::FieldModifiers::PUBLIC;
    }
    if access_flags.contains(FieldAccessFlags::PRIVATE) {
        bits |= rajac_types::FieldModifiers::PRIVATE;
    }
    if access_flags.contains(FieldAccessFlags::PROTECTED) {
        bits |= rajac_types::FieldModifiers::PROTECTED;
    }
    if access_flags.contains(FieldAccessFlags::STATIC) {
        bits |= rajac_types::FieldModifiers::STATIC;
    }
    if access_flags.contains(FieldAccessFlags::FINAL) {
        bits |= rajac_types::FieldModifiers::FINAL;
    }
    if access_flags.contains(FieldAccessFlags::VOLATILE) {
        bits |= rajac_types::FieldModifiers::VOLATILE;
    }
    if access_flags.contains(FieldAccessFlags::TRANSIENT) {
        bits |= rajac_types::FieldModifiers::TRANSIENT;
    }

    rajac_types::FieldModifiers(bits)
}

fn resolve_class_relationships(
    parsed_classes: &[ParsedClass],
    symbol_table: &mut SymbolTable,
) -> RajacResult<()> {
    // First pass: Collect all the relationships we need to resolve (only read from symbol_table)
    let relationships: Vec<_> = parsed_classes
        .iter()
        .filter_map(|parsed_class| {
            let package_table = symbol_table.get_package_shared(&parsed_class.package)?;
            let symbol = package_table.get(&parsed_class.class_name)?;
            let type_id = symbol.ty;

            let super_type_id = parsed_class
                .super_class
                .as_ref()
                .and_then(|super_class_name| {
                    find_type_id_for_class_impl(super_class_name, symbol_table)
                });

            let interface_type_ids: Vec<rajac_types::TypeId> = parsed_class
                .interfaces
                .iter()
                .filter_map(|interface_name| {
                    find_type_id_for_class_impl(interface_name, symbol_table)
                })
                .collect();

            Some((
                type_id,
                super_type_id,
                interface_type_ids,
                parsed_class.methods.clone(),
                parsed_class.fields.clone(),
            ))
        })
        .collect();

    // Second pass: Apply the relationships (only write to type_arena)
    let class_lookup = build_class_lookup(symbol_table);
    let primitive_lookup = symbol_table.primitive_types().clone();
    let (type_arena, method_arena, field_arena) = symbol_table.arenas_mut();
    for (type_id, super_type_id, interface_type_ids, methods, fields) in relationships {
        let mut resolved_methods = Vec::with_capacity(methods.len());
        for method in methods {
            let params = method
                .params
                .iter()
                .map(|param| {
                    resolve_field_type(param, &primitive_lookup, &class_lookup, type_arena)
                })
                .collect::<Vec<_>>();
            let return_type = match &method.return_type {
                Some(field_type) => {
                    resolve_field_type(field_type, &primitive_lookup, &class_lookup, type_arena)
                }
                None => void_type_id(&primitive_lookup),
            };
            let signature = rajac_types::MethodSignature {
                name: method.name.clone(),
                params,
                return_type,
                throws: Vec::new(),
                modifiers: method.modifiers,
            };
            let method_id = method_arena.alloc(signature);
            resolved_methods.push((method.name, method_id));
        }

        let mut resolved_fields = Vec::with_capacity(fields.len());
        for field in fields {
            let field_type =
                resolve_field_type(&field.ty, &primitive_lookup, &class_lookup, type_arena);
            let signature =
                rajac_types::FieldSignature::new(field.name.clone(), field_type, field.modifiers);
            let field_id = field_arena.alloc(signature);
            resolved_fields.push((field.name, field_id));
        }

        let class_type = type_arena.get_mut(type_id);
        if let rajac_types::Type::Class(class_type_mut) = class_type {
            class_type_mut.superclass = super_type_id;
            class_type_mut.interfaces = interface_type_ids;
            for (name, method_id) in resolved_methods {
                class_type_mut.add_method(name, method_id);
            }
            for (name, field_id) in resolved_fields {
                class_type_mut.add_field(name, field_id);
            }
        }
    }

    Ok(())
}

fn find_type_id_for_class_impl(
    class_name: &str,
    symbol_table: &SymbolTable,
) -> Option<rajac_types::TypeId> {
    let (package, simple_name) = if let Some(last_dot) = class_name.rfind('.') {
        (
            SharedString::new(&class_name[..last_dot]),
            SharedString::new(&class_name[last_dot + 1..]),
        )
    } else {
        (SharedString::new(""), SharedString::new(class_name))
    };

    symbol_table.lookup_type_id(package.as_str(), simple_name.as_str())
}

fn build_class_lookup(symbol_table: &SymbolTable) -> HashMap<String, rajac_types::TypeId> {
    let mut lookup = HashMap::new();
    for (package, table) in symbol_table.iter() {
        for (name, symbol) in table.iter() {
            let fqn = if package.is_empty() {
                name.as_str().to_string()
            } else {
                format!("{}.{}", package.as_str(), name.as_str())
            };
            lookup.insert(fqn, symbol.ty);
        }
    }
    lookup
}

fn resolve_field_type(
    field_type: &FieldType,
    primitive_lookup: &HashMap<SharedString, rajac_types::TypeId>,
    class_lookup: &HashMap<String, rajac_types::TypeId>,
    type_arena: &mut rajac_types::TypeArena,
) -> rajac_types::TypeId {
    match field_type {
        FieldType::Base(base_type) => primitive_lookup
            .get(&SharedString::new(primitive_name_from_base_type(base_type)))
            .copied()
            .unwrap_or(rajac_types::TypeId::INVALID),
        FieldType::Object(class_name) => {
            let fqn = class_name.replace('/', ".");
            class_lookup
                .get(&fqn)
                .copied()
                .unwrap_or(rajac_types::TypeId::INVALID)
        }
        FieldType::Array(component_type) => {
            let element_type =
                resolve_field_type(component_type, primitive_lookup, class_lookup, type_arena);
            if element_type == rajac_types::TypeId::INVALID {
                rajac_types::TypeId::INVALID
            } else {
                type_arena.alloc(rajac_types::Type::array(element_type))
            }
        }
    }
}

fn primitive_name_from_base_type(base_type: &BaseType) -> &'static str {
    match base_type {
        BaseType::Boolean => "boolean",
        BaseType::Byte => "byte",
        BaseType::Char => "char",
        BaseType::Short => "short",
        BaseType::Int => "int",
        BaseType::Long => "long",
        BaseType::Float => "float",
        BaseType::Double => "double",
    }
}

fn void_type_id(
    primitive_lookup: &HashMap<SharedString, rajac_types::TypeId>,
) -> rajac_types::TypeId {
    primitive_lookup
        .get(&SharedString::new("void"))
        .copied()
        .unwrap_or(rajac_types::TypeId::INVALID)
}

use rajac_base::result::{RajacResult, ResultExt};
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_class_file_name() {
        let classpath = Classpath::new();
        assert!(classpath.entries.is_empty());
    }

    #[test]
    fn method_name_handles_init_and_clinit() {
        let class_name = SharedString::new("Widget");
        assert_eq!(
            method_name_for_class("<init>", &class_name),
            Some(SharedString::new("Widget"))
        );
        assert_eq!(method_name_for_class("<clinit>", &class_name), None);
        assert_eq!(
            method_name_for_class("run", &class_name),
            Some(SharedString::new("run"))
        );
    }

    #[test]
    fn resolves_field_types_with_lookup_and_arrays() {
        let mut symbol_table = SymbolTable::new();
        let primitive_lookup = symbol_table.primitive_types().clone();
        let type_arena = symbol_table.type_arena_mut();
        let string_id = type_arena.alloc(rajac_types::Type::class(
            rajac_types::ClassType::new(SharedString::new("String"))
                .with_package(SharedString::new("java.lang")),
        ));
        let mut class_lookup = HashMap::new();
        class_lookup.insert("java.lang.String".to_string(), string_id);

        let object_type = FieldType::Object("java/lang/String".to_string());
        assert_eq!(
            resolve_field_type(&object_type, &primitive_lookup, &class_lookup, type_arena),
            string_id
        );

        let int_type = FieldType::Base(BaseType::Int);
        let int_id = resolve_field_type(&int_type, &primitive_lookup, &class_lookup, type_arena);
        assert_eq!(
            type_arena.get(int_id),
            &rajac_types::Type::primitive(rajac_types::PrimitiveType::Int)
        );

        let array_type = FieldType::Array(Box::new(FieldType::Base(BaseType::Boolean)));
        let array_id =
            resolve_field_type(&array_type, &primitive_lookup, &class_lookup, type_arena);
        match type_arena.get(array_id) {
            rajac_types::Type::Array(array) => {
                let element_type = type_arena.get(array.element_type);
                assert_eq!(
                    element_type,
                    &rajac_types::Type::primitive(rajac_types::PrimitiveType::Boolean)
                );
            }
            other => panic!("expected array type, got {other:?}"),
        }
    }

    #[test]
    fn maps_access_flags_to_method_modifiers() {
        let flags =
            MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC | MethodAccessFlags::FINAL;
        let modifiers = method_modifiers_from_access_flags(flags);
        assert_eq!(
            modifiers,
            rajac_types::MethodModifiers(
                rajac_types::MethodModifiers::PUBLIC
                    | rajac_types::MethodModifiers::STATIC
                    | rajac_types::MethodModifiers::FINAL
            )
        );
    }

    #[test]
    fn maps_access_flags_to_field_modifiers() {
        let flags = FieldAccessFlags::PUBLIC | FieldAccessFlags::STATIC | FieldAccessFlags::FINAL;
        let modifiers = field_modifiers_from_access_flags(flags);
        assert_eq!(
            modifiers,
            rajac_types::FieldModifiers(
                rajac_types::FieldModifiers::PUBLIC
                    | rajac_types::FieldModifiers::STATIC
                    | rajac_types::FieldModifiers::FINAL
            )
        );
    }
}
