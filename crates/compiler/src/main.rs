use rajac_compiler::{Compiler, CompilerConfig};
use std::path::Path;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let source_dir = Path::new(&dir);

    let config = CompilerConfig {
        source_dir: source_dir.to_path_buf(),
        target_dir: source_dir.join("classes"),
    };
    let mut compiler = Compiler::new(config);

    if let Err(e) = compiler.compile_directory() {
        eprintln!("Compilation failed: {:?}", e);
        std::process::exit(1);
    }

    println!("Compiled successfully");
}
