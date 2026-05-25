//! Compile `examples/rules.yaml` into `$OUT_DIR/rules_table.rs` at build
//! time. Per ADR 0005 / `docs/architecture/03-rule-model.md`.
//!
//! Re-runs when `examples/rules.yaml` changes (Cargo's `rerun-if-changed`)
//! AND when any rule-pipeline source changes (because they define the
//! types the generated code references).

#[path = "src/rules/compiler.rs"]
mod compiler;

#[path = "src/rules/types.rs"]
pub mod types;

use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let rules_yaml = manifest_dir.join("examples/rules.yaml");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let out_file = out_dir.join("rules_table.rs");

    println!("cargo:rerun-if-changed={}", rules_yaml.display());
    println!("cargo:rerun-if-changed=src/rules/compiler.rs");
    println!("cargo:rerun-if-changed=src/rules/types.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let yaml_text = std::fs::read_to_string(&rules_yaml).unwrap_or_else(|e| {
        panic!(
            "thurward build.rs: cannot read {}: {e}\n\
             (this file is the source of truth for the compiled rule table;\n\
             see docs/architecture/03-rule-model.md)",
            rules_yaml.display()
        );
    });

    let source = compiler::compile(&yaml_text).unwrap_or_else(|e| {
        panic!(
            "thurward build.rs: compiling {} failed: {e}\n\
             (the YAML is also validated by CI's `schema-validate` job\n\
             against schemas/rules.schema.json — check that first)",
            rules_yaml.display()
        );
    });

    std::fs::write(&out_file, &source).unwrap_or_else(|e| {
        panic!(
            "thurward build.rs: cannot write {}: {e}",
            out_file.display()
        );
    });
}

// `compiler.rs` and `types.rs` are also `mod`-included from `src/lib.rs`,
// but `build.rs` runs before the main crate is built so it cannot depend
// on `crate::*` — we re-include both via `#[path = ...]` above. The
// `compiler::` -> `crate::rules::types::` references in the *emitted* code
// resolve against the main crate at compile time, not against this script.
//
// To keep that pun working, `compiler.rs` references the types via
// `crate::rules::types::*` paths (correct for src/) — we shadow those
// here with the `super::types` path so this script's own type-checking
// passes. This works because `compiler.rs` only uses the types in
// *function signatures*, never in literal codegen output.

// Re-export the script-local `types` module under the path
// `crate::rules::types::*` that compiler.rs expects. `build.rs` has no
// `crate::rules` namespace, so we provide one.
mod rules {
    pub use super::types;
}
