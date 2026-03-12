/* 📖 # Why separate discovery into its own stage?
File discovery is a distinct phase that happens before any parsing.
It's responsible for finding all Java source files in the source directory.
Separating this makes it easier to test file discovery logic independently
and allows for potential future extensions like filtering by patterns,
excluding certain directories, or handling different source file types.
*/

use rajac_base::result::RajacResult;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Discovers all Java source files in the given directory.
pub fn find_java_files(dir: &Path) -> RajacResult<Vec<PathBuf>> {
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
