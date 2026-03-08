use rajac_lexer::Lexer;
use std::path::Path;
use walkdir::WalkDir;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ballpit".to_string());
    let path = Path::new(&dir);

    let mut total_tokens = 0;

    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let file_path = entry.path();
        if file_path.extension().is_some_and(|ext| ext == "java") {
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", file_path.display(), e);
                    continue;
                }
            };

            let tokens = Lexer::new(&source).count();
            total_tokens += tokens;
        }
    }

    println!("Total tokens: {}", total_tokens);
}
