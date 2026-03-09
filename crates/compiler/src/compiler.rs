use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::classfile::generate_classfiles;
use rajac_parser::parse;
use ristretto_classfile::attributes::Attribute;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Compiler struct that handles compilation of Java source files
pub struct Compiler {
    // Configuration and state can be added here
}

impl Compiler {
    /// Create a new Compiler instance
    pub fn new() -> Self {
        Compiler {
            // Initialize any state here
        }
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    /// Compile all Java files in a source directory to a target directory
    pub fn compile_directory(&self, source_dir: &Path, target_dir: &Path) -> RajacResult<()> {
        // Create target directory if it doesn't exist
        fs::create_dir_all(target_dir).context("Failed to create target directory")?;

        // Find all Java files in source directory
        let java_files = self.find_java_files(source_dir)?;

        if java_files.is_empty() {
            return Ok(());
        }

        // Compile each file
        for java_file in &java_files {
            self.compile_file(java_file, target_dir)?;
        }

        Ok(())
    }

    /// Find all Java files in a directory
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

    /// Compile a single Java file
    fn compile_file(&self, source_file: &Path, target_dir: &Path) -> RajacResult<()> {
        // Read source file
        let source = fs::read_to_string(source_file).context("Failed to read source file")?;

        // Parse the source
        let parse_result = parse(&source);

        // Generate class files
        let mut class_files = generate_classfiles(&parse_result.ast, &parse_result.arena)?;

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

        // Write class files to target directory
        let classes_dir = target_dir.join("classes");

        for class_file in class_files {
            let class_name = class_file
                .constant_pool
                .try_get_class(class_file.this_class)
                .context("Failed to get class name from constant pool")?;

            let class_path = classes_dir.join(format!("{}.class", class_name));

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

        Ok(())
    }
}
