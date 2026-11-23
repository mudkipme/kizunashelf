use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let values_path = Path::new("src/secrets/bundled/values.rs");
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR not set by Cargo");
    let dest = Path::new(&out_dir).join("bundled_values.rs");

    if values_path.exists() {
        println!("cargo:rerun-if-changed=src/secrets/bundled/values.rs");
        fs::write(
            dest,
            "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/secrets/bundled/values.rs\"));\n",
        )
        .expect("failed to write bundled_values.rs");
    } else {
        println!("cargo:rerun-if-changed=src/secrets/bundled");
        fs::write(dest, "pub(super) const VALUES: &[(&str, &str)] = &[];\n")
            .expect("failed to write bundled_values.rs");
    }
}
