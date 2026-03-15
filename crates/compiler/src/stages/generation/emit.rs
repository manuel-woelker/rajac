use super::{GenerationResult, generation_diagnostics_for_unsupported_features};
use rajac_ast::{Ast, AstArena};
use rajac_base::file_path::FilePath;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::classfile::generate_classfiles_with_report as bytecode_generate_classfiles;
use rajac_symbols::SymbolTable;
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::Path;

pub(crate) fn emit_classfiles(
    ast: &Ast,
    arena: &AstArena,
    type_arena: &rajac_types::TypeArena,
    symbol_table: &SymbolTable,
    source_file: &FilePath,
    target_dir: &Path,
) -> RajacResult<GenerationResult> {
    let generated = bytecode_generate_classfiles(ast, arena, type_arena, symbol_table)?;
    let diagnostics = generation_diagnostics_for_unsupported_features(
        source_file,
        ast.source.as_str(),
        &generated.unsupported_features,
    );

    let mut class_files = generated.class_files;

    for class_file in &mut class_files {
        let source_file_attribute_index = class_file.constant_pool.add_utf8("SourceFile")?;
        let source_file_index = class_file
            .constant_pool
            .add_utf8(source_file.file_name().unwrap_or("unknown"))
            .with_context(|| {
                format!(
                    "Failed to add SourceFile attribute for source file '{}'",
                    source_file.as_str()
                )
            })?;
        class_file.attributes.push(Attribute::SourceFile {
            name_index: source_file_attribute_index,
            source_file_index,
        });
    }

    let class_count = class_files.len();

    for class_file in class_files {
        let class_name = class_file
            .constant_pool
            .try_get_class(class_file.this_class)
            .with_context(|| {
                format!(
                    "Failed to get generated class name from constant pool for source file '{}'",
                    source_file.as_str()
                )
            })?;

        let class_path = target_dir.join(format!("{}.class", class_name));

        if let Some(parent) = class_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create package directory '{}' for class '{}'",
                    parent.display(),
                    class_name
                )
            })?;
        }

        let mut bytes = Vec::new();
        class_file
            .to_bytes(&mut bytes)
            .with_context(|| format!("Failed to serialize generated class '{}'", class_name))?;
        fs::write(&class_path, &bytes).with_context(|| {
            format!(
                "Failed to write class file '{}' for class '{}'",
                class_path.display(),
                class_name
            )
        })?;
    }

    Ok(GenerationResult {
        class_count,
        diagnostics,
    })
}
