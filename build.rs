use std::env;
use std::path::Path;
use syntect::dumps::dump_to_file;
use syntect::parsing::SyntaxSet;

fn main() {
    println!("cargo:rustc-cfg=tracing_unstable");
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
}
