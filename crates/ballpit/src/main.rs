use rayon::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let base_dir = "ballpit";
    let files_per_package = 1000;
    let num_packages = 10;

    std::fs::remove_dir_all(base_dir).ok();
    std::fs::create_dir_all(base_dir).unwrap();

    // Generate file data in parallel
    let file_data: Vec<(String, String)> = (0..num_packages)
        .into_par_iter()
        .flat_map(|pkg_idx| {
            (0..files_per_package)
                .into_par_iter()
                .map(move |file_idx| {
                    let global_idx = pkg_idx * files_per_package + file_idx;
                    let pkg_dir = format!("{}/pkg{:03}", base_dir, pkg_idx);
                    let file_name = format!("Main{:03}.java", file_idx);
                    let file_path = format!("{}/{}", pkg_dir, file_name);

                    let content = format!(
                        "package pkg{:03};\n public class Main{:03} {{\n    public static void main(String[] args) {{\n        System.out.println({});\n    }}\n}}\n",
                        pkg_idx, file_idx, global_idx
                    );

                    (file_path, content)
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Create directories sequentially (needed for file system operations)
    for pkg_idx in 0..num_packages {
        let pkg_dir = format!("{}/pkg{:03}", base_dir, pkg_idx);
        std::fs::create_dir_all(&pkg_dir).unwrap();
    }

    println!("Created {} package directories", num_packages);

    // Write files in parallel
    file_data.par_iter().for_each(|(file_path, content)| {
        std::fs::write(file_path, content).unwrap();
    });

    println!("Wrote {} Java files in parallel", file_data.len());

    // Write sources.txt sequentially with all file paths
    let mut sources_txt = File::create(Path::new(base_dir).join("sources.txt")).unwrap();
    for (file_path, _) in &file_data {
        writeln!(sources_txt, "{}", file_path).unwrap();
    }

    println!(
        "Generated {} Java files in {}",
        num_packages * files_per_package,
        base_dir
    );
}
