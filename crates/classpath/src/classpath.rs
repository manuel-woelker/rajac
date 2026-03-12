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
                    let package = symbol_table.package(&parsed.package);
                    let name = SharedString::new(parsed.class_name.clone());
                    let kind = if parsed.is_interface {
                        SymbolKind::Interface
                    } else {
                        SymbolKind::Class
                    };
                    package.insert(parsed.class_name, Symbol::new(name, kind));
                }
            }
        }

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
        let parsed_classes: Vec<(String, String, bool)> = class_data
            .into_par_iter()
            .filter_map(|bytes| {
                let class_file = ClassFile::from_bytes(&mut Cursor::new(bytes)).ok()?;
                parse_class_file(&class_file).map(|p| (p.package, p.class_name, p.is_interface))
            })
            .collect();
        println!(
            "Read {:?} in {}ms ({} classes)",
            jar,
            start.elapsed().as_millis(),
            parsed_classes.len()
        );

        for (package, class_name, is_interface) in parsed_classes {
            let package = symbol_table.package(&package);
            let name = SharedString::new(class_name.clone());
            let kind = if is_interface {
                SymbolKind::Interface
            } else {
                SymbolKind::Class
            };
            package.insert(class_name, Symbol::new(name, kind));
        }

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

    Some(ParsedClass {
        package,
        class_name,
        is_interface,
    })
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
