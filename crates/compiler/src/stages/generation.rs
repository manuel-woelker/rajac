/* 📖 # Why separate generation into its own stage?
Code generation is the final phase where ASTs are converted to bytecode.
This involves creating class files with proper attributes, constants,
and method implementations. Separating this stage allows for focused
testing of bytecode generation and potential optimization of the
output without affecting other compilation phases.
*/

use crate::CompilationUnit;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_base::file_path::FilePath;
use rajac_bytecode::classfile::generate_classfiles as bytecode_generate_classfiles;
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::Path;

/// Generates class files from compilation units.
pub fn generate_classfiles(
    compilation_units: &[CompilationUnit],
    target_dir: &Path,
) -> RajacResult<usize> {
    let mut total_files = 0;

    for unit in compilation_units {
        let count = emit_classfiles(&unit.ast, &unit.arena, &unit.source_file, target_dir)?;
        total_files += count;
    }

    Ok(total_files)
}

/// Emits class files for a single compilation unit.
fn emit_classfiles(
    ast: &rajac_ast::Ast,
    arena: &rajac_ast::AstArena,
    source_file: &FilePath,
    target_dir: &Path,
) -> RajacResult<usize> {
    let mut class_files = bytecode_generate_classfiles(ast, arena)?;

    for class_file in &mut class_files {
        let source_file_attribute_index = class_file.constant_pool.add_utf8("SourceFile")?;
        let source_file_index = class_file
            .constant_pool
            .add_utf8(source_file.file_name().unwrap_or("unknown").to_string())?;
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
