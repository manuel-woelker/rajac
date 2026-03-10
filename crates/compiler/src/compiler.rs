use rajac_ast::{Ast, AstArena, ClassKind};
use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::classfile::generate_classfiles;
use rajac_parser::parse;
use rajac_symbols::{Symbol, SymbolKind, SymbolTable};
use rayon::prelude::*;
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct CompilationUnit {
    pub source_file: PathBuf,
    pub parse_result: ParseResult,
}

type ParseResult = rajac_parser::ParseResult;

#[allow(dead_code)]
pub struct Compiler {
    symbol_table: SymbolTable,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            symbol_table: SymbolTable::new(),
        }
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    pub fn compile_directory(&self, source_dir: &Path, target_dir: &Path) -> RajacResult<()> {
        fs::create_dir_all(target_dir).context("Failed to create target directory")?;

        let java_files = self.find_java_files(source_dir)?;

        if java_files.is_empty() {
            return Ok(());
        }

        let compilation_units: Vec<CompilationUnit> = java_files
            .par_iter()
            .map(|java_file| {
                let source = fs::read_to_string(java_file).context("Failed to read source file")?;
                let parse_result = parse(&source);
                Ok(CompilationUnit {
                    source_file: java_file.clone(),
                    parse_result,
                })
            })
            .collect::<RajacResult<Vec<_>>>()?;

        let mut symbol_table = SymbolTable::new();
        for unit in &compilation_units {
            populate_symbol_table(
                &mut symbol_table,
                &unit.parse_result.ast,
                &unit.parse_result.arena,
            );
        }

        let results: Vec<RajacResult<usize>> = compilation_units
            .par_iter()
            .map(|unit| {
                emit_classfiles(
                    &unit.parse_result.ast,
                    &unit.parse_result.arena,
                    &unit.source_file,
                    target_dir,
                )
            })
            .collect();

        let mut total_classfiles = 0;
        for result in results {
            total_classfiles += result?;
        }

        println!(
            "Compiled {} Java files -> {} class files",
            java_files.len(),
            total_classfiles
        );

        Ok(())
    }

    fn find_java_files(&self, dir: &Path) -> RajacResult<Vec<PathBuf>> {
        let mut java_files = Vec::new();

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "java") {
                java_files.push(path.to_path_buf());
            }
        }

        Ok(java_files)
    }
}

fn populate_symbol_table(symbol_table: &mut SymbolTable, ast: &Ast, arena: &AstArena) {
    let package_name = ast
        .package
        .as_ref()
        .map(|p| {
            p.name
                .segments
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_default();

    let package = symbol_table.package(&package_name);

    for class_id in &ast.classes {
        let class = arena.class_decl(*class_id);
        let name = class.name.0.clone();
        let kind = match class.kind {
            ClassKind::Class => SymbolKind::Class,
            ClassKind::Interface => SymbolKind::Interface,
            ClassKind::Enum | ClassKind::Record | ClassKind::Annotation => continue,
        };
        package.insert(name.to_string(), Symbol::new(name, kind));
    }
}

fn emit_classfiles(
    ast: &Ast,
    arena: &AstArena,
    source_file: &Path,
    target_dir: &Path,
) -> RajacResult<usize> {
    let mut class_files = generate_classfiles(ast, arena)?;

    for class_file in &mut class_files {
        let source_file_attribute_index = class_file.constant_pool.add_utf8("SourceFile")?;
        let source_file_index = class_file
            .constant_pool
            .add_utf8(source_file.file_name().unwrap().display().to_string())?;
        class_file.attributes.push(Attribute::SourceFile {
            name_index: source_file_attribute_index,
            source_file_index,
        })
    }

    let classfile_count = class_files.len();

    for class_file in class_files {
        let class_name = class_file
            .constant_pool
            .try_get_class(class_file.this_class)
            .context("Failed to get class name from constant pool")?;

        let class_path = target_dir.join(format!("{}.class", class_name));

        if let Some(parent) = class_path.parent() {
            fs::create_dir_all(parent).context("Failed to create package directory")?;
        }

        let mut bytes = Vec::new();
        class_file.to_bytes(&mut bytes)?;
        fs::write(&class_path, &bytes).context(format!(
            "Failed to write class file: {}",
            class_path.display()
        ))?;
    }

    Ok(classfile_count)
}
