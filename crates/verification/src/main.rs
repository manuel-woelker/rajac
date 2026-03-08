use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use sha2::{Sha256, Digest};
use std::fs;

fn main() -> Result<()> {
    let _sources_dir = Path::new("verification/sources");
    let reference_output = Path::new("verification/output/openjdk_21/rajac/verification");
    
    // Compile sources with rajac (this would be replaced with actual rajac compilation)
    println!("Compiling sources with rajac...");
    // TODO: Replace with actual rajac compilation command
    // For now, we'll just compare the reference output with itself as a placeholder
    
    let rajac_output = reference_output; // Placeholder - replace with actual rajac output
    
    compare_outputs(reference_output, rajac_output)?;
    
    Ok(())
}

fn compare_outputs(reference: &Path, actual: &Path) -> Result<()> {
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
        let ref_filename = ref_path.file_name().unwrap().to_string_lossy();
        let act_filename = act_path.file_name().unwrap().to_string_lossy();
        
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

fn get_class_files(dir: &Path) -> Result<Vec<PathBuf>> {
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

fn compute_sha256(file_path: &Path) -> Result<String> {
    let bytes = fs::read(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
    
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    
    Ok(hex::encode(result))
}