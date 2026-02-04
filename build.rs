use std::env;
use std::path::Path;
use syntect::dumps::dump_to_file;
use syntect::parsing::SyntaxSet;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("syntax_set.bin");

    // Start with default syntaxes
    let default_syntax_set = SyntaxSet::load_defaults_newlines();
    let mut builder = default_syntax_set.into_builder();

    // Add custom syntaxes from the syntaxes directory
    if Path::new("syntaxes").exists() {
        builder
            .add_from_folder(Path::new("syntaxes"), true)
            .expect("Failed to load syntaxes from folder");
    }

    let syntax_set = builder.build();

    // Serialize the SyntaxSet to a binary file
    dump_to_file(&syntax_set, &dest_path).expect("Failed to save SyntaxSet");

    println!("cargo:rerun-if-changed=syntaxes/");

    // Extract dependency versions for runtime display
    if let Ok(metadata) = cargo_metadata::MetadataCommand::new().exec() {
        for pkg in &metadata.packages {
            match pkg.name.as_str() {
                "nu-protocol" => {
                    println!("cargo:rustc-env=NU_VERSION={}", pkg.version);
                }
                "cross-stream" => {
                    println!("cargo:rustc-env=XS_VERSION={}", pkg.version);
                }
                _ => {}
            }
        }
    }
}
