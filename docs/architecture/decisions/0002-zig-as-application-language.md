# ADR 0002 — Zig as application language (superseded)

**Status:** Superseded by [ADR 0017](0017-hermit-rust-substrate.md)
**Date:** 2026-05-23
**Superseded:** 2026-05-25
**Deciders:** magicletur

> **Superseded.** thurward is implemented in **Rust** — see
> [ADR 0017](0017-hermit-rust-substrate.md). The original Zig
> selection made sense when the firewall application was a thin
> filter hook on top of `lib-lwip`; after
> [ADR 0013](0013-zig-fast-path-on-uknetdev.md) moved the entire
> data plane (parser, conntrack, NAT, TX) into the app, the
> memory-safety / ecosystem trade-off shifted to Rust.

## What survives from this ADR

The reasoning that survives is the "why *some* compile-checked
systems language" stance:

- A firewall data plane is parser-heavy, hash-table-heavy, and
  raw-pointer-adjacent. Picking a language that catches bounds and
  use-after-free at compile time pays meaningful rent in this domain.
- Whichever language ships also drives the build system, the
  dependency pinning story, and the reproducible-build closure
  ([ADR 0010](0010-supply-chain-hardening.md)). The build system
  needs to support `no_std`-style targets and a single-binary
  cross-compile workflow.

Both apply to Rust as much as they did to Zig. The choice of *which*
safety-providing systems language is what changed.

## Why the choice flipped

- **The amount of code we own grew.** ADR 0013 took ownership of the
  entire data path. Rust's ownership/lifetime guarantees cover that
  ground more tightly than Zig's compile-time bounds checks do.
- **smoltcp** (a maintained `no_std` Rust TCP/IP crate) supplies
  fuzzed wire-format parsers and L4 state types that we'd otherwise
  hand-roll. We use only its `wire` modules, not its socket or
  interface layers; this is consistent with the
  "app-owns-the-data-plane" stance from ADR 0013.
- **Zig pre-1.0 churn** was a real maintenance tax. The
  [`ziglang/zig#20546`](https://github.com/ziglang/zig/issues/20546)
  workaround mentioned in earlier drafts of this ADR is no longer
  relevant.
- **Hiring / contributor onboarding.** More Rust systems engineers
  exist than Zig systems engineers.

## Rejected alternatives (status carries over)

- **C.** Still rejected for the same reason: every safety-relevant
  bug in a firewall is high-stakes; want a safety net beyond
  hand-discipline.
- **Go.** Still rejected — garbage collection, large runtime,
  doesn't fit a unikernel substrate.
- **Zig (this ADR's original choice).** Now superseded as above.
