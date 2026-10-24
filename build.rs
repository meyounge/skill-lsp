#[path = "src/fnd.rs"]
#[allow(dead_code)]
mod fnd;
use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=SKILL_EMBED_FND_DIR");
    println!("cargo:rerun-if-changed=src/fnd.rs");
    let entries =
        if let Some(root) = env::var_os("SKILL_EMBED_FND_DIR").filter(|root| !root.is_empty()) {
            let root = PathBuf::from(root);
            println!("cargo:rerun-if-changed={}", root.display());
            let entries = fnd::load_dir(&root)
                .unwrap_or_else(|e| panic!("Cannot embed {}: {e}", root.display()));
            println!(
                "cargo:warning=Embedding {} local SKILL functions",
                entries.len()
            );
            entries
        } else {
            Vec::new()
        };
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("embedded.json"),
        serde_json::to_vec(&entries).unwrap(),
    )
    .unwrap();
}
