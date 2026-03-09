use rajac_compiler::Compiler;
use std::path::Path;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let source_dir = Path::new(&dir);

    let compiler = Compiler::new();

    if let Err(e) = compiler.compile_directory(source_dir, source_dir) {
        eprintln!("Compilation failed: {:?}", e);
        std::process::exit(1);
    }

    println!("Compiled successfully");
}
