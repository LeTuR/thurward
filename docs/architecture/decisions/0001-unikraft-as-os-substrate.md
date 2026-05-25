# ADR 0001 — Unikraft as OS substrate

**Status:** Accepted (substrate clause restored by
[ADR 0018](0018-substrate-pivot-unikraft.md); language clause
superseded by [ADR 0017](0017-hermit-rust-substrate.md) — original
Zig choice replaced with Rust)
**Date:** 2026-05-23
**Superseded:** 2026-05-25 (substrate clause originally) —
2026-05-25 (substrate clause restored)
**Deciders:** magicletur

> **Substrate clause restored.** thurward ships on **Unikraft** for
> v1, with Rust as the application language (per
> [ADR 0017](0017-hermit-rust-substrate.md)'s language clause) and
> `lib-uknetdev` for raw frame access (per
> [ADR 0018](0018-substrate-pivot-unikraft.md)). The original
> language clause of this ADR (Zig) is superseded by ADR 0017; the
> substrate clause was briefly superseded by ADR 0017 and restored
> by ADR 0018 after phase-1 implementation found Hermit was the
> wrong substrate shape for a forwarding device. The original
> alternatives analysis below (why not stripped Linux, why not
> Alpine + Rust, etc.) still applies.

## What survives from this ADR

The unikernel-minimalism stance:

- Tiny attack surface — only the libraries linked are present. No
  shell, no package manager, no kernel modules, no userspace
  utilities.
- Millisecond boot times make launcher-restart-on-rule-change cheap,
  which is what makes build-time rule compilation
  ([ADR 0005](0005-build-time-rule-compilation.md)) viable in the
  first place.
- Single static image, easy to sign and reproduce
  ([ADR 0010](0010-supply-chain-hardening.md)), trivially attestable.
- Forces the firewall logic to *be* the entire program, rather than
  gluing together kernel features — leads to a more comprehensible
  system.

These properties were the reason for picking a unikernel over a
general-purpose OS, and they apply to Hermit just as well as they did
to Unikraft.

## Rejected alternatives (still rejected)

- **Stripped Linux + nftables + eBPF/XDP.** Most mature, best
  observability tooling out of the box, easiest to operate. Rejected
  because the kernel attack surface (millions of lines) and the
  standard userspace baggage fight the minimalism goal directly. A
  50 MB Alpine image is still ~50× the bytes of a unikernel image.
- **MirageOS.** Has `qubes-mirage-firewall` — a real, decade-old
  production firewall unikernel. Strongest alternative on the
  prior-art axis. Rejected because OCaml-only; Rust interop would
  be more work than just using a Rust-native unikernel
  ([ADR 0017](0017-hermit-rust-substrate.md) picks Hermit for this).
- **A small Rust/Go binary on Alpine.** Pragmatic, observable, but
  the Linux kernel below it dominates the attack surface — defeats
  the purpose of choosing minimalism.

## What did not survive — and why

The specific Unikraft choice did not survive ADR 0017. The reasons:

- After [ADR 0013](0013-zig-fast-path-on-uknetdev.md) moved the data
  plane into the application, the language-vs-framework trade-off
  shifted toward a Rust-native unikernel; Hermit fits that better
  than Unikraft does (Unikraft's value-add is mostly its C library
  catalog, which we're not using).
- An intermediate revision of this ADR (earlier on 2026-05-25)
  lifted bare-metal x86_64 EFI direct boot to a v1 first-class
  target. ADR 0017 walks that back because Hermit's bare-metal
  story is less mature than Unikraft's; bare-metal is deferred to
  v2. See [09 — Limitations](../09-limitations.md).
