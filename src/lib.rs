//! thurward — minimalistic unikernel firewall.
//!
//! This crate is split into the **rule pipeline** (host-buildable, used
//! by `build.rs`) and the **dataplane** (added in later PRs, target
//! `x86_64-unknown-hermit`). PR A lands only the rule pipeline.
//!
//! See `docs/architecture/` for the design that this code implements.

pub mod rules;
