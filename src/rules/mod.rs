//! Rule pipeline — compile-time and runtime.
//!
//! - [`types`] holds the runtime types (`Rule`, `Action`, `CidrV4`, ...).
//!   These are shared between `build.rs` and the dataplane.
//! - [`compiler`] turns `examples/rules.yaml` into Rust source. Used by
//!   `build.rs`; testable from integration tests.

pub mod compiler;
pub mod types;
