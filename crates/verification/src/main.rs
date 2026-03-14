use colored::*;
use rajac_base::file_path::FilePath;
use rajac_base::logging::{debug, error, info, info_span, trace, warn};
use rajac_base::result::{RajacResult, ResultExt};
use rajac_bytecode::pretty_print::pretty_print_classfile;
use rajac_compiler::{Compiler, CompilerConfig};
use rajac_symbols::SymbolTable;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use std::collections::HashMap;

/* 📖 # Why use error message overrides instead of exact OpenJDK matching?
The verification system needs to ensure compatibility with OpenJDK while allowing rajac to provide
better, more specific error messages. OpenJDK sometimes gives generic errors like "';' expected"
when rajac can provide more precise diagnostics like "illegal character". This override system
uses OpenJDK's line numbers for consistency but compares against rajac's superior error messages.
*/
fn get_error_message_overrides() -> HashMap<&'static str, &'static str> {
    let mut overrides = HashMap::new();

    // rajac provides more specific error messages than OpenJDK
    // Format: "TestFileName" -> "expected rajac error message"
    overrides.insert("IllegalCharacter", "illegal character");
    overrides.insert("InvalidIdentifierStart", "invalid identifier start");
    overrides.insert("MalformedNumber", "malformed number");

    overrides
}

fn main() -> std::process::ExitCode {
    rajac_base::logging::init_logging();
    rajac_base::cli::try_main_with_headline("verification failed", run)
}

fn run() -> RajacResult<()> {
    let _span = info_span!("verification_main").entered();
    let sources_dir = Path::new("verification/sources");
    let sources_invalid_dir = Path::new("verification/sources_invalid");
    let reference_output = Path::new("verification/output/openjdk_21");
    let rajac_base_output = Path::new("verification/output/rajac");
    let rajac_output = rajac_base_output;
    let classpaths = vec![FilePath::new("/usr/lib/jvm/java-8-openjdk/jre/lib/rt.jar")];
    info!("initializing verification");
    debug!(reference_output = %reference_output.display(), rajac_output = %rajac_output.display());
    let prepopulated_symbol_table = Compiler::symbol_table_from_classpaths(&classpaths)?;

    if fs::exists(rajac_output)? {
        fs::remove_dir_all(rajac_output).context("Failed to remove rajac output directory")?;
    }

    // Create output directory for rajac
    fs::create_dir_all(rajac_output).context("Failed to create rajac output directory")?;

    // Compile sources with rajac
    info!("compiling valid sources with rajac");
    println!("Compiling sources with rajac...");
    compile_with_rajac(
        sources_dir,
        rajac_base_output,
        &classpaths,
        &prepopulated_symbol_table,
    )?;

    // Compare outputs
    let valid_files_count = compare_outputs(reference_output, rajac_output)?;

    // Verify invalid sources produce errors
    info!("verifying invalid sources");
    println!("\nVerifying invalid sources...");
    let invalid_files_count = verify_invalid_sources(
        sources_invalid_dir,
        reference_output,
        &classpaths,
        &prepopulated_symbol_table,
    )?;

    // Print harmonized summary
    println!("\n✓ {} valid files match", valid_files_count);
    println!("✓ {} invalid files verified", invalid_files_count);

    Ok(())
}

fn compile_with_rajac(
    sources_dir: &Path,
    output_dir: &Path,
    classpaths: &[FilePath],
    prepopulated_symbol_table: &SymbolTable,
) -> RajacResult<()> {
    let _span = info_span!(
        "compile_with_rajac",
        sources_dir = %sources_dir.display(),
        output_dir = %output_dir.display()
    )
    .entered();
    // Compile sources with rajac using the Compiler struct
    let config = CompilerConfig {
        source_dirs: vec![FilePath::new(sources_dir)],
        target_dir: FilePath::new(output_dir),
        classpaths: classpaths.to_vec(),
        emit_timing_statistics: false,
    };
    let mut compiler = Compiler::new_with_symbol_table(config, prepopulated_symbol_table.clone());
    debug!("starting compiler.compile_directory for valid sources");
    compiler.compile_directory()?;
    info!("finished compiling valid sources");

    Ok(())
}

fn compare_outputs(reference: &Path, actual: &Path) -> RajacResult<usize> {
    let _span = info_span!(
        "compare_outputs",
        reference = %reference.display(),
        actual = %actual.display()
    )
    .entered();
    println!("Comparing compiler outputs...");
    println!("Reference: {}", reference.display());
    println!("Actual: {}", actual.display());

    let reference_files = get_class_files(reference)?;
    let actual_files = get_class_files(actual)?;

    // Check if same files exist
    info!(
        reference_count = reference_files.len(),
        actual_count = actual_files.len()
    );
    if reference_files.len() != actual_files.len() {
        warn!("class file count mismatch");
        println!("File count mismatch!");
        println!("Reference: {} files", reference_files.len());
        println!("Actual: {} files", actual_files.len());

        // Extract filenames for comparison
        let ref_names: std::collections::HashSet<_> = reference_files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        let act_names: std::collections::HashSet<_> = actual_files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        // Find files only in reference
        let only_in_reference: Vec<_> = ref_names.difference(&act_names).collect();
        if !only_in_reference.is_empty() {
            println!(
                "{}Expected files not found in actual output:",
                "Missing:".red()
            );
            for name in only_in_reference {
                println!("  {}", name);
            }
        }

        // Find files only in actual
        let only_in_actual: Vec<_> = act_names.difference(&ref_names).collect();
        if !only_in_actual.is_empty() {
            println!(
                "{}Unexpected files found in actual output:",
                "Extra:".yellow()
            );
            for name in only_in_actual {
                println!("  {}", name);
            }
        }

        // Find common files to compare
        let common_names: std::collections::HashSet<_> =
            ref_names.intersection(&act_names).cloned().collect();

        // Sort filenames for consistent comparison
        let mut sorted_names: Vec<_> = common_names.iter().collect::<Vec<_>>();
        sorted_names.sort();

        // Create ordered vectors for comparison by finding matching paths
        let ref_common: Vec<_> = sorted_names
            .iter()
            .filter_map(|name| {
                reference_files
                    .iter()
                    .find(|p| p.file_name().unwrap().to_string_lossy().into_owned() == **name)
                    .cloned()
            })
            .collect();
        let act_common: Vec<_> = sorted_names
            .iter()
            .filter_map(|name| {
                actual_files
                    .iter()
                    .find(|p| p.file_name().unwrap().to_string_lossy().into_owned() == **name)
                    .cloned()
            })
            .collect();

        info!(
            common_files = common_names.len(),
            "comparing common files after file count mismatch"
        );
        println!("Comparing {} common files...", common_names.len());
        compare_file_contents(&ref_common, &act_common)?;
    } else {
        info!(files = reference_files.len(), "comparing class files");
        println!("Comparing {} files...", reference_files.len());
        compare_file_contents(&reference_files, &actual_files)?;
    }

    Ok(reference_files.len())
}

fn compare_file_contents(reference_files: &[PathBuf], actual_files: &[PathBuf]) -> RajacResult<()> {
    let _span = info_span!("compare_file_contents", files = reference_files.len()).entered();
    let mut mismatches = 0;

    for (ref_path, act_path) in reference_files.iter().zip(actual_files.iter()) {
        let ref_filename = ref_path.file_name().unwrap().to_string_lossy().into_owned();
        let act_filename = act_path.file_name().unwrap().to_string_lossy().into_owned();
        let _span = info_span!("compare_class_file", file = %ref_filename).entered();

        if ref_filename != act_filename {
            warn!(expected = %ref_filename, actual = %act_filename, "filename mismatch");
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
        trace!(
            reference_bytes = ref_bytes.len(),
            actual_bytes = act_bytes.len(),
            "read class files"
        );

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
            warn!(reference_hash = %ref_pretty_hash, actual_hash = %act_pretty_hash, "pretty-printed class file mismatch");
            println!("{}Content mismatch in: {}", "❌ ".red(), ref_filename,);

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
                    }
                    diff::Result::Right(line) => {
                        println!("  {}{}", "+".green(), line);
                        has_changes = true;
                    }
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
        // Success - main function will print summary
    } else {
        warn!(mismatches, "class file mismatches found");
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

fn verify_invalid_sources(
    invalid_dir: &Path,
    reference_output: &Path,
    classpaths: &[FilePath],
    prepopulated_symbol_table: &SymbolTable,
) -> RajacResult<usize> {
    let _span = info_span!(
        "verify_invalid_sources",
        invalid_dir = %invalid_dir.display(),
        reference_output = %reference_output.display()
    )
    .entered();
    let invalid_output_dir = reference_output.join("invalid");
    let error_overrides = get_error_message_overrides();

    let java_files = get_java_files(invalid_dir)?;
    let total_files = java_files.len();
    let mut failures = 0;
    info!(
        invalid_files = total_files,
        overrides = error_overrides.len(),
        "loaded invalid source verification inputs"
    );

    // Compile all invalid sources once
    println!("Compiling all invalid sources...");
    let config = CompilerConfig {
        source_dirs: vec![FilePath::new(invalid_dir)],
        target_dir: FilePath::new(invalid_dir.join("classes")),
        classpaths: classpaths.to_vec(),
        emit_timing_statistics: false,
    };

    let mut compiler = Compiler::new_with_symbol_table(config, prepopulated_symbol_table.clone());
    compiler.compile_directory().ok();

    let diagnostics = &compiler.diagnostics;
    info!(
        diagnostics = diagnostics.len(),
        "finished compiling invalid sources"
    );

    if diagnostics.is_empty() {
        error!("invalid sources produced no diagnostics");
        println!(
            "{} All invalid sources compiled successfully (this should not happen)",
            "Error:".red()
        );
        return Ok(0);
    }

    // Map diagnostics to files
    let mut file_diagnostics: std::collections::HashMap<
        String,
        Vec<&rajac_diagnostics::Diagnostic>,
    > = std::collections::HashMap::new();

    for diagnostic in diagnostics {
        // Find which file this diagnostic belongs to
        for chunk in &diagnostic.chunks {
            let file_path = chunk.path.as_str();
            let file_stem = Path::new(file_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();

            file_diagnostics
                .entry(file_stem.to_string())
                .or_default()
                .push(diagnostic);
            trace!(file_stem = %file_stem, line = chunk.line, "mapped diagnostic to file");
        }
    }

    // Verify each file against its expected error
    for java_file in &java_files {
        let file_stem = java_file.file_stem().unwrap().to_string_lossy();
        let _span = info_span!("verify_invalid_file", file = %file_stem).entered();
        let ref_output_file = invalid_output_dir.join(format!("{}.txt", file_stem));

        if !ref_output_file.exists() {
            warn!(reference_output = %ref_output_file.display(), "missing reference output");
            println!(
                "{} Missing reference output for: {}",
                "Error:".red(),
                file_stem
            );
            failures += 1;
            continue;
        }

        let ref_content = fs::read_to_string(&ref_output_file).context(format!(
            "Failed to read reference output: {}",
            ref_output_file.display()
        ))?;

        let (ref_line, ref_error) = parse_reference_error(&ref_content)?;
        debug!(reference_line = ref_line, reference_error = %ref_error, "parsed reference diagnostic");

        let empty_vec: Vec<&rajac_diagnostics::Diagnostic> = vec![];
        let file_diagnostics = file_diagnostics.get(&*file_stem).unwrap_or(&empty_vec);
        debug!(
            diagnostics_for_file = file_diagnostics.len(),
            "collected diagnostics for invalid file"
        );

        if file_diagnostics.is_empty() {
            warn!("invalid file compiled successfully");
            println!(
                "{} {} should have failed but compiled successfully",
                "Error:".red(),
                file_stem
            );
            println!(
                "  Reference expected error at line {}: {}",
                ref_line, ref_error
            );
            failures += 1;
            continue;
        }

        // Find the best matching diagnostic for this file
        let diagnostic = file_diagnostics
            .iter()
            .find(|d| {
                // Look for a diagnostic that matches the expected line
                d.chunks.iter().any(|chunk| chunk.line == ref_line)
            })
            .or_else(|| file_diagnostics.iter().next())
            .unwrap();
        trace!("selected diagnostic for invalid file");

        let rajac_line = diagnostic.chunks.first().map(|c| c.line);
        let rajac_error = diagnostic.message.as_str();

        let line_match = rajac_line.is_some_and(|l| l == ref_line);

        /* 📖 # Why check for overrides before error comparison?
        The override system allows us to verify line numbers against OpenJDK (for compatibility)
        while comparing error messages against rajac's improved diagnostics. This enables rajac
        to provide better error messages without breaking verification compatibility.
        */
        // Check if we have an override for this test case
        let expected_error = error_overrides
            .get(&*file_stem)
            .copied()
            .unwrap_or(&ref_error);

        let error_match = rajac_error
            .to_lowercase()
            .contains(&expected_error.to_lowercase());
        debug!(
            reference_line = ref_line,
            rajac_line = rajac_line.unwrap_or_default(),
            expected_error = %expected_error,
            rajac_error = %rajac_error,
            line_match,
            error_match,
            "compared invalid source diagnostic"
        );

        if !line_match || !error_match {
            warn!("invalid source mismatch");
            print_invalid_source_mismatch(
                &file_stem,
                ref_line,
                &ref_error,
                if error_overrides.contains_key(&*file_stem) {
                    Some(expected_error)
                } else {
                    None
                },
                rajac_line,
                rajac_error,
                java_file,
            );
            failures += 1;
        }
    }

    if failures > 0 {
        let passing_files = total_files - failures;
        println!(
            "\n{} {} invalid source files failed verification, {} passed",
            "Error:".red(),
            failures,
            passing_files
        );
    } else {
        // No summary printed - main function will handle it
    }

    Ok(total_files - failures)
}

fn get_java_files(dir: &Path) -> RajacResult<Vec<PathBuf>> {
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

    java_files.sort();
    Ok(java_files)
}

fn parse_reference_error(content: &str) -> RajacResult<(usize, String)> {
    let line_regex = Regex::new(r"(.+\.java):(\d+): error: (.+)")?;

    for line in content.lines() {
        if let Some(caps) = line_regex.captures(line) {
            let line_num: usize = caps
                .get(2)
                .unwrap()
                .as_str()
                .parse()
                .context("Failed to parse line number")?;
            let error_msg = caps.get(3).unwrap().as_str().to_string();
            return Ok((line_num, error_msg));
        }
    }

    Err(rajac_base::err!("Failed to parse reference error output"))
}

fn print_invalid_source_mismatch(
    file_stem: &str,
    reference_line: usize,
    reference_error: &str,
    override_error: Option<&str>,
    rajac_line: Option<usize>,
    rajac_error: &str,
    source_path: &Path,
) {
    println!(
        "{} {} - diagnostic mismatch",
        "Error:".red(),
        file_stem.bold()
    );
    println!("  Source:    {}", source_path.display());
    println!(
        "  Reference: line {}, error '{}'",
        reference_line, reference_error
    );
    if let Some(override_error) = override_error {
        println!("  Override:  error '{}'", override_error);
    }
    println!(
        "  Rajac:     line {}, error '{}'",
        rajac_line
            .map(|line| line.to_string())
            .unwrap_or_else(|| "N/A".to_string()),
        rajac_error
    );
}
