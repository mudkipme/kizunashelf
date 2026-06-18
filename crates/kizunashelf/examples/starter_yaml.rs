//! Prints the "Media Library" starter vault config as YAML — the source for
//! `config/vault-config.example.yaml`. Regenerate that file with:
//!
//! ```sh
//! cargo run -p kizunashelf --example starter_yaml > config/vault-config.example.yaml
//! ```

fn main() {
    print!(
        "# Generated from the \"Media Library\" starter template in\n\
         # crates/kizunashelf/src/templates.rs — do not edit by hand. Regenerate with:\n\
         #   cargo run -p kizunashelf --example starter_yaml > config/vault-config.example.yaml\n\
         {}",
        kizunashelf::templates::starter_vault_config_yaml()
    );
}
