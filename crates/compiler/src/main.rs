use rajac_compiler::{Compiler, CompilerConfig};
use rajac_base::file_path::FilePath;
use std::path::Path;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let source_dir = Path::new(&dir);

    let config = CompilerConfig {
        source_dir: FilePath::new(source_dir),
        target_dir: FilePath::new(source_dir.join("classes")),
    };
    let mut compiler = Compiler::new(config);

    if let Err(e) = compiler.compile_directory() {
        eprintln!("Compilation failed: {:?}", e);
        std::process::exit(1);
    }

    println!("Compiled successfully");
}
