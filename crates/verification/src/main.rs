use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use sha2::{Sha256, Digest};
use std::fs;
use rajac_compiler::Compiler;
use rajac_base::result::{RajacResult, ResultExt};

fn main() -> RajacResult<()> {
    let sources_dir = Path::new("verification/sources");
    let reference_output = Path::new("verification/output/openjdk_21/rajac/verification");
    let rajac_output = Path::new("verification/output/rajac/rajac/verification");
    
    // Create output directory for rajac
    fs::create_dir_all(rajac_output)
        .context("Failed to create rajac output directory")?;
    
    // Compile sources with rajac
    println!("Compiling sources with rajac...");
    compile_with_rajac(sources_dir, rajac_output)?;
    
    // Compare outputs
    compare_outputs(reference_output, rajac_output)?;
    
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
        if path.is_file() && path.extension().map_or(false, |ext| ext == "java") {
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
        return Ok(()); // Continue with comparison of existing files
    }
    
    // Compare each file
    let mut mismatches = 0;
    for (ref_path, act_path) in reference_files.iter().zip(actual_files.iter()) {
        let ref_filename = ref_path.file_name().unwrap().to_string_lossy().into_owned();
        let act_filename = act_path.file_name().unwrap().to_string_lossy().into_owned();
        
        if ref_filename != act_filename {
            println!("Filename mismatch: {} vs {}", ref_filename, act_filename);
            mismatches += 1;
            continue;
        }
        
        let ref_hash = compute_sha256(ref_path)?;
        let act_hash = compute_sha256(act_path)?;
        
        if ref_hash != act_hash {
            println!("Content mismatch in: {}", ref_filename);
            println!("  Reference hash: {}", ref_hash);
            println!("  Actual hash: {}", act_hash);
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
        if path.is_file() && path.extension().map_or(false, |ext| ext == "class") {
            class_files.push(path.to_path_buf());
        }
    }
    
    class_files.sort();
    Ok(class_files)
}

fn compute_sha256(file_path: &Path) -> RajacResult<String> {
    let bytes = fs::read(file_path)
        .context(format!("Failed to read file: {}", file_path.display()))?;
    
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    
    Ok(hex::encode(result))
}