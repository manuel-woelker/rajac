use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let base_dir = "ballpit";
    let files_per_package = 1000;
    let num_packages = 100;

    std::fs::remove_dir_all(base_dir).unwrap();
    std::fs::create_dir_all(base_dir).unwrap();

    for pkg_idx in 0..num_packages {
        let pkg_dir = format!("{}/pkg{:03}", base_dir, pkg_idx);
        std::fs::create_dir_all(&pkg_dir).unwrap();

        for file_idx in 0..files_per_package {
            let global_idx = pkg_idx * files_per_package + file_idx;
            let file_name = format!("Main{:03}.java", file_idx);
            let file_path = format!("{}/{}", pkg_dir, file_name);

            let content = format!(
                "package pkg{:03};\n public class Main{:03} {{\n    public static void main(String[] args) {{\n        System.out.println({});\n    }}\n}}\n",
                pkg_idx, file_idx, global_idx
            );

            std::fs::write(&file_path, content).unwrap();
        }
        println!("Wrote pkg {pkg_dir}");
    }

    let mut sources_txt = File::create(Path::new(base_dir).join("sources.txt")).unwrap();
    for pkg_idx in 0..num_packages {
        let pkg_dir = format!("{}/pkg{:03}", base_dir, pkg_idx);

        for file_idx in 0..files_per_package {
            writeln!(sources_txt, "{}/Main{:03}.java", pkg_dir, file_idx).unwrap();
        }
    }

    println!(
        "Generated {} Java files in {}",
        num_packages * files_per_package,
        base_dir
    );
}
