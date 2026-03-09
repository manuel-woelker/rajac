use colored::*;
use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::pretty_print::pretty_print_classfile;
use rajac_compiler::Compiler;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() -> RajacResult<()> {
    let sources_dir = Path::new("verification/sources");
    let reference_output = Path::new("verification/output/openjdk_21/rajac/verification");
    let rajac_base_output = Path::new("verification/output/rajac/rajac/verification");
    let rajac_output = rajac_base_output.join("classes");

    // Create output directory for rajac
    fs::create_dir_all(&rajac_output).context("Failed to create rajac output directory")?;

    // Compile sources with rajac
    println!("Compiling sources with rajac...");
    compile_with_rajac(sources_dir, rajac_base_output)?;

    // Compare outputs
    compare_outputs(reference_output, &rajac_output)?;

    Ok(())
}

fn compile_with_rajac(sources_dir: &Path, output_dir: &Path) -> RajacResult<()> {
    // Find all Java files in sources directory
    let mut java_files = Vec::new();
    for entry in WalkDir::new(sources_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "java") {
            java_files.push(path.to_path_buf());
        }
    }

    // Compile each file with rajac using the Compiler struct
    let compiler = Compiler::new();
    compiler.compile_directory(sources_dir, output_dir)?;

    println!("Compiled {} files with rajac", java_files.len());
    Ok(())
}

fn compare_outputs(reference: &Path, actual: &Path) -> RajacResult<()> {
    println!("Comparing compiler outputs...");
    println!("Reference: {}", reference.display());
    println!("Actual: {}", actual.display());

    let reference_files = get_class_files(reference)?;
    let actual_files = get_class_files(actual)?;

    // Check if same files exist
    if reference_files.len() != actual_files.len() {
        println!("File count mismatch!");
        println!("Reference: {} files", reference_files.len());
        println!("Actual: {} files", actual_files.len());
        
        // Extract filenames for comparison
        let ref_names: std::collections::HashSet<_> = reference_files.iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        let act_names: std::collections::HashSet<_> = actual_files.iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        
        // Find files only in reference
        let only_in_reference: Vec<_> = ref_names.difference(&act_names).collect();
        if !only_in_reference.is_empty() {
            println!("{}Expected files not found in actual output:", "Missing:".red());
            for name in only_in_reference {
                println!("  {}", name);
            }
        }
        
        // Find files only in actual
        let only_in_actual: Vec<_> = act_names.difference(&ref_names).collect();
        if !only_in_actual.is_empty() {
            println!("{}Unexpected files found in actual output:", "Extra:".yellow());
            for name in only_in_actual {
                println!("  {}", name);
            }
        }
        
        // Find common files to compare
        let common_names: std::collections::HashSet<_> = ref_names.intersection(&act_names).cloned().collect();
        
        // Sort filenames for consistent comparison
        let mut sorted_names: Vec<_> = common_names.iter().collect::<Vec<_>>();
        sorted_names.sort();
        
        // Create ordered vectors for comparison by finding matching paths
        let ref_common: Vec<_> = sorted_names.iter()
            .filter_map(|name| {
                reference_files.iter().find(|p| {
                    p.file_name().unwrap().to_string_lossy().into_owned() == **name
                }).cloned()
            })
            .collect();
        let act_common: Vec<_> = sorted_names.iter()
            .filter_map(|name| {
                actual_files.iter().find(|p| {
                    p.file_name().unwrap().to_string_lossy().into_owned() == **name
                }).cloned()
            })
            .collect();
        
        println!("Comparing {} common files...", common_names.len());
        compare_file_contents(&ref_common, &act_common)?;
    } else {
        println!("Comparing {} files...", reference_files.len());
        compare_file_contents(&reference_files, &actual_files)?;
    }

    Ok(())
}

fn compare_file_contents(reference_files: &[PathBuf], actual_files: &[PathBuf]) -> RajacResult<()> {
    let mut mismatches = 0;
    
    for (ref_path, act_path) in reference_files.iter().zip(actual_files.iter()) {
        let ref_filename = ref_path.file_name().unwrap().to_string_lossy().into_owned();
        let act_filename = act_path.file_name().unwrap().to_string_lossy().into_owned();

        if ref_filename != act_filename {
            println!("Filename mismatch: {} vs {}", ref_filename, act_filename);
            mismatches += 1;
            continue;
        }

        // Read and pretty print both class files for comparison
        let ref_bytes = fs::read(ref_path).context(format!(
            "Failed to read reference file: {}",
            ref_path.display()
        ))?;
        let act_bytes = fs::read(act_path).context(format!(
            "Failed to read actual file: {}",
            act_path.display()
        ))?;

        // Parse class files and pretty print them
        use std::io::Cursor;

        let ref_class_file: ristretto_classfile::ClassFile =
            ristretto_classfile::ClassFile::from_bytes(&mut Cursor::new(&ref_bytes))
                .context("Failed to parse reference class file")?;
        let act_class_file: ristretto_classfile::ClassFile =
            ristretto_classfile::ClassFile::from_bytes(&mut Cursor::new(&act_bytes))
                .context("Failed to parse actual class file")?;

        let ref_pretty = pretty_print_classfile(&ref_class_file);
        let act_pretty = pretty_print_classfile(&act_class_file);

        // Compare hashes of pretty-printed output instead of raw bytecode
        let ref_pretty_hash = {
            let mut hasher = Sha256::new();
            hasher.update(ref_pretty.as_bytes());
            hex::encode(hasher.finalize())
        };
        let act_pretty_hash = {
            let mut hasher = Sha256::new();
            hasher.update(act_pretty.as_bytes());
            hex::encode(hasher.finalize())
        };

        if ref_pretty_hash != act_pretty_hash {
            println!(
                "{}Content mismatch in: {}",
                "❌ ".red(),
                ref_filename,
            );

            // Generate diff
            let ref_text = ref_pretty.as_str();
            let act_text = act_pretty.as_str();

            let diff = diff::lines(ref_text, act_text);

            let mut has_changes = false;
            for change in diff {
                match change {
                    diff::Result::Left(line) => {
                        println!("  {}{}", "-".red(), line);
                        has_changes = true;
                    },
                    diff::Result::Right(line) => {
                        println!("  {}{}", "+".green(), line);
                        has_changes = true;
                    },
                    diff::Result::Both(_line, _) => (), // Nothing to emit,
                }
            }
            
            if !has_changes {
                println!("  {} No differences in pretty-printed output (bytecode differs only in implementation details)", "Note:".yellow());
            }

            mismatches += 1;
        }
    }

    if mismatches == 0 {
        println!("✓ All files match!");
    } else {
        println!("✗ Found {} mismatches", mismatches);
    }

    Ok(())
}

fn get_class_files(dir: &Path) -> RajacResult<Vec<PathBuf>> {
    let mut class_files = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "class") {
            class_files.push(path.to_path_buf());
        }
    }

    class_files.sort();
    Ok(class_files)
}

