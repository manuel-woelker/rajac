use rajac_base::shared_string::SharedString;
use rajac_symbols::{Symbol, SymbolKind, SymbolTable};
use rayon::prelude::*;
use ristretto_classfile::ClassFile;
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
    package: String,
    class_name: String,
    is_interface: bool,
    super_class: Option<String>,
    interfaces: Vec<String>,
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

    pub fn add_to_symbol_table(
        &self,
        symbol_table: &mut SymbolTable,
        type_arena: &mut rajac_types::TypeArena,
    ) -> RajacResult<()> {
        for entry in &self.entries {
            match entry {
                ClasspathEntry::Directory(dir) => {
                    self.add_directory_to_symbol_table(dir, symbol_table, type_arena)?;
                }
                ClasspathEntry::Jar(jar) => {
                    self.add_jar_to_symbol_table(jar, symbol_table, type_arena)?;
                }
            }
        }
        Ok(())
    }

    fn add_directory_to_symbol_table(
        &self,
        dir: &Path,
        symbol_table: &mut SymbolTable,
        type_arena: &mut rajac_types::TypeArena,
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

        // Second pass: Create all classes in symbol table and type arena
        for parsed_class in &parsed_classes {
            let package = symbol_table.package(&parsed_class.package);
            let name = SharedString::new(parsed_class.class_name.clone());
            let kind = if parsed_class.is_interface {
                SymbolKind::Interface
            } else {
                SymbolKind::Class
            };

            // Create the appropriate type in the TypeArena (without superclass/interfaces for now)
            let class_type = if !parsed_class.package.is_empty() {
                rajac_types::ClassType::new(parsed_class.class_name.clone())
                    .with_package(parsed_class.package.clone())
            } else {
                rajac_types::ClassType::new(parsed_class.class_name.clone())
            };
            let type_id = type_arena.alloc(rajac_types::Type::class(class_type));

            package.insert(
                parsed_class.class_name.clone(),
                Symbol::new(name, kind, type_id),
            );
        }

        // Third pass: Resolve superclass and interface relationships
        resolve_class_relationships(&parsed_classes, symbol_table, type_arena)?;

        Ok(())
    }

    fn add_jar_to_symbol_table(
        &self,
        jar: &Path,
        symbol_table: &mut SymbolTable,
        type_arena: &mut rajac_types::TypeArena,
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

        // First pass: Create all classes in symbol table and type arena
        for parsed_class in &parsed_classes {
            let package = symbol_table.package(&parsed_class.package);
            let name = SharedString::new(parsed_class.class_name.clone());
            let kind = if parsed_class.is_interface {
                SymbolKind::Interface
            } else {
                SymbolKind::Class
            };

            // Create the appropriate type in the TypeArena (without superclass/interfaces for now)
            let class_type = if !parsed_class.package.is_empty() {
                rajac_types::ClassType::new(parsed_class.class_name.clone())
                    .with_package(parsed_class.package.clone())
            } else {
                rajac_types::ClassType::new(parsed_class.class_name.clone())
            };
            let type_id = type_arena.alloc(rajac_types::Type::class(class_type));

            package.insert(
                parsed_class.class_name.clone(),
                Symbol::new(name, kind, type_id),
            );
        }

        // Second pass: Resolve superclass and interface relationships
        resolve_class_relationships(&parsed_classes, symbol_table, type_arena)?;

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
            internal_name[..last_slash].replace('/', "."),
            internal_name[last_slash + 1..].to_string(),
        )
    } else {
        (String::new(), internal_name.to_string())
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
            .map(|name| name.replace('/', "."))
    } else {
        None
    };

    // Extract interface information
    let interfaces: Vec<String> = class_file
        .interfaces
        .iter()
        .filter_map(|&interface_idx| {
            class_file
                .constant_pool
                .try_get_class(interface_idx)
                .ok()
                .map(|name| name.replace('/', "."))
        })
        .collect();

    Some(ParsedClass {
        package,
        class_name,
        is_interface,
        super_class,
        interfaces,
    })
}

fn resolve_class_relationships(
    parsed_classes: &[ParsedClass],
    symbol_table: &SymbolTable,
    type_arena: &mut rajac_types::TypeArena,
) -> RajacResult<()> {
    // First pass: Collect all the relationships we need to resolve
    let mut relationships = Vec::new();
    for parsed_class in parsed_classes {
        if let Some(package_table) = symbol_table.get_package(&parsed_class.package)
            && let Some(symbol) = package_table.get(&parsed_class.class_name)
        {
            let super_type_id = if let Some(super_class_name) = &parsed_class.super_class {
                find_type_id_for_class(super_class_name, symbol_table, type_arena)
            } else {
                None
            };

            let interface_type_ids: Vec<rajac_types::TypeId> = parsed_class
                .interfaces
                .iter()
                .filter_map(|interface_name| {
                    find_type_id_for_class(interface_name, symbol_table, type_arena)
                })
                .collect();

            relationships.push((symbol.ty, super_type_id, interface_type_ids));
        }
    }

    // Second pass: Apply the relationships
    for (type_id, super_type_id, interface_type_ids) in relationships {
        let class_type = type_arena.get_mut(type_id);
        if let rajac_types::Type::Class(class_type_mut) = class_type {
            class_type_mut.superclass = super_type_id;
            class_type_mut.interfaces = interface_type_ids;
        }
    }

    Ok(())
}

fn find_type_id_for_class(
    class_name: &str,
    symbol_table: &SymbolTable,
    type_arena: &rajac_types::TypeArena,
) -> Option<rajac_types::TypeId> {
    // Parse the class name to extract package and simple name
    let (package, simple_name) = if let Some(last_dot) = class_name.rfind('.') {
        (
            class_name[..last_dot].to_string(),
            class_name[last_dot + 1..].to_string(),
        )
    } else {
        (String::new(), class_name.to_string())
    };

    // Look up the class in the symbol table
    if let Some(package_table) = symbol_table.get_package(&package)
        && let Some(symbol) = package_table.get(&simple_name)
    {
        // Verify this is actually a class type
        let class_type = type_arena.get(symbol.ty);
        if let rajac_types::Type::Class(_) = class_type {
            return Some(symbol.ty);
        }
    }

    None
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
}
