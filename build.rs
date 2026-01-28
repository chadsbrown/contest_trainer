use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let contest_dir: PathBuf = [manifest_dir.as_str(), "src", "contest"].iter().collect();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", contest_dir.display());

    let mut contest_modules: Vec<String> = fs::read_dir(&contest_dir)
        .expect("failed to read src/contest directory")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                return None;
            }
            let file_name = path.file_name()?.to_str()?.to_string();
            let stem = path.file_stem()?.to_str()?.to_string();

            let excluded = matches!(file_name.as_str(), "mod.rs" | "types.rs" | "callsign.rs");
            if excluded {
                return None;
            }

            Some(stem)
        })
        .collect();

    contest_modules.sort();

    let mut output = String::new();
    for module in &contest_modules {
        let abs_path: PathBuf = [contest_dir.to_str().unwrap(), &format!("{}.rs", module)]
            .iter()
            .collect();
        let path_literal = abs_path.to_string_lossy().replace('\\', "\\\\");
        output.push_str(&format!("#[path = \"{}\"]\n", path_literal));
        output.push_str(&format!("pub mod {};\n", module));
    }

    output.push_str("\npub fn generated_contest_registry() -> Vec<ContestDescriptor> {\n");
    output.push_str("    vec![\n");
    for module in &contest_modules {
        output.push_str(&format!(
            "        ContestDescriptor {{ id: {0}::CONTEST_ID, display_name: {0}::DISPLAY_NAME, factory: {0}::make_contest }},\n",
            module
        ));
    }
    output.push_str("    ]\n");
    output.push_str("}\n");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path: PathBuf = [out_dir.as_str(), "contest_registry.rs"].iter().collect();
    fs::write(&out_path, output).expect("failed to write contest_registry.rs");
}
