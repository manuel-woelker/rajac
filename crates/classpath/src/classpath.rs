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
    package: SharedString,
    class_name: SharedString,
    is_interface: bool,
    super_class: Option<SharedString>,
    interfaces: Vec<SharedString>,
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
    symbol_table: &mut SymbolTable,
) -> RajacResult<()> {
    // First pass: Collect all the relationships we need to resolve (only read from symbol_table)
    let relationships: Vec<_> = parsed_classes
        .iter()
        .filter_map(|parsed_class| {
            let package_table = symbol_table.get_package(&parsed_class.package)?;
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

            Some((type_id, super_type_id, interface_type_ids))
        })
        .collect();

    // Second pass: Apply the relationships (only write to type_arena)
    let type_arena = symbol_table.type_arena_mut();
    for (type_id, super_type_id, interface_type_ids) in relationships {
        let class_type = type_arena.get_mut(type_id);
        if let rajac_types::Type::Class(class_type_mut) = class_type {
            class_type_mut.superclass = super_type_id;
            class_type_mut.interfaces = interface_type_ids;
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

    // Look up the class in the symbol table
    if let Some(package_table) = symbol_table.get_package(&package)
        && let Some(symbol) = package_table.get(&simple_name)
    {
        return Some(symbol.ty);
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
