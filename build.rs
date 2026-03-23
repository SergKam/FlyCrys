/// Pre-compile custom `.sublime-syntax` files into a binary pack so the
/// runtime never needs the `yaml-load` feature.
fn main() {
    println!("cargo:rerun-if-changed=assets/syntaxes");

    let mut builder = syntect::parsing::SyntaxSetBuilder::new();
    builder
        .add_from_folder("assets/syntaxes", true)
        .expect("failed to load custom sublime-syntax files from assets/syntaxes");
    let ss = builder.build();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = format!("{out_dir}/custom_syntaxes.packdump");
    syntect::dumps::dump_to_uncompressed_file(&ss, &path)
        .expect("failed to write custom_syntaxes.packdump");
}
