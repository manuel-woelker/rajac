//! # File Discovery Stage
//!
//! This module handles the first stage of the compilation pipeline: discovering
//! Java source files in the specified directory structure.
//!
//! ## Purpose
//!
//! The discovery stage is responsible for:
//! - Scanning directory trees recursively
//! - Identifying Java source files by extension
//! - Building a complete list of files to be compiled
//! - Handling symbolic links and directory traversal safely
//!
//! ## Implementation Details
//!
//! Uses the `walkdir` crate for efficient directory traversal with:
//! - Symbolic link following (when safe)
//! - Parallel traversal capabilities
//! - Proper error handling for inaccessible files
//!
//! ## Usage
//!
//! This stage is typically called from the main compiler pipeline but can
//! be used independently for file inspection or testing purposes.
//!
//! ```rust,no_run,ignore
//! use rajac_compiler::stages::discovery;
//! use std::path::Path;
//!
//! let source_dir = Path::new("src/main/java");
//! let java_files = discovery::find_java_files(source_dir)?;
//! println!("Found {} Java files", java_files.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

/* 📖 # Why separate discovery into its own stage?
File discovery is a distinct phase that happens before any parsing.
It's responsible for finding all Java source files in the source directory.
Separating this makes it easier to test file discovery logic independently
and allows for potential future extensions like filtering by patterns,
excluding certain directories, or handling different source file types.
*/

use rajac_base::file_path::FilePath;
use rajac_base::logging::instrument;
use rajac_base::result::{RajacResult, ResultExt};
use std::path::Path;
use walkdir::WalkDir;

/// Discovers all Java source files in the given directory.
///
/// This function recursively scans the specified directory and all subdirectories
/// to find files with the `.java` extension. It follows symbolic links
/// when they are safe to follow.
///
/// # Parameters
///
/// - `dir` - The root directory to search for Java source files
///
/// # Returns
///
/// A `Vec<FilePath>` containing paths to all discovered Java files.
/// The files are returned in the order they are discovered by the
/// directory traversal (which may not be sorted).
///
/// # Errors
///
/// Returns an error if:
/// - The source directory cannot be accessed
/// - There are permission issues during traversal
/// - Symbolic links create infinite loops (detected by walkdir)
///
/// # Examples
///
/// ```rust,no_run,ignore
/// use rajac_compiler::stages::discovery;
/// use std::path::Path;
///
/// let source_dir = Path::new("src");
/// let java_files = discovery::find_java_files(source_dir)?;
///
/// for file in &java_files {
///     println!("Found Java file: {}", file.as_str());
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Notes
///
/// - Files are identified solely by the `.java` extension
/// - No content validation is performed at this stage
/// - Hidden files and directories are included if they have `.java` extension
/// - The function is case-sensitive for file extensions
#[instrument(name = "compiler.phase.discovery", skip(dir), fields(source_dir = %dir.display()))]
pub fn find_java_files(dir: &Path) -> RajacResult<Vec<FilePath>> {
    let mut java_files = Vec::new();

    for entry in WalkDir::new(dir).follow_links(true) {
        let entry = entry.with_context(|| {
            format!(
                "Failed to traverse '{}' while discovering Java files in '{}'",
                dir.display(),
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "java") {
            java_files.push(FilePath::new(path));
        }
    }

    Ok(java_files)
}

#[cfg(test)]
mod tests {
    use super::find_java_files;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn discovery_errors_when_followed_symlink_is_broken() {
        let root = unique_temp_dir("discovery_broken_symlink");
        fs::create_dir_all(&root).unwrap();

        let broken_link = root.join("missing.java");
        std::os::unix::fs::symlink(root.join("does-not-exist.java"), &broken_link).unwrap();

        let error = find_java_files(&root).unwrap_err();
        let rendered = error.to_test_string();

        assert!(rendered.contains("Failed to traverse"));
        assert!(rendered.contains(root.to_string_lossy().as_ref()));

        fs::remove_dir_all(&root).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rajac_{name}_{nanos}"))
    }
}
