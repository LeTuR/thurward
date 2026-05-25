# ADR 0017 — Hermit unikernel + Rust + smoltcp wire layer

**Status:** Accepted (substrate clause superseded by
[ADR 0018](0018-substrate-pivot-unikraft.md); language clause and
smoltcp::wire clause stay live)
**Date:** 2026-05-25
**Deciders:** magicletur
**Supersedes:** [ADR 0001](0001-unikraft-as-os-substrate.md) (language clause only — substrate clause restored by [ADR 0018](0018-substrate-pivot-unikraft.md)),
[ADR 0002](0002-zig-as-application-language.md) (language)
**Partially superseded by:** [ADR 0018](0018-substrate-pivot-unikraft.md)
(the Hermit substrate clause is replaced with Unikraft;
phase-1 implementation found that upstream `hermit-os/kernel`
does not expose a public raw-frame API, which is incompatible
with the app-owns-the-data-plane stance — see ADR 0018 for
the rationale)

## Context

[ADR 0013](0013-zig-fast-path-on-uknetdev.md) committed the
application to owning the entire data path — parser, conntrack, NAT,
TX scheduling — with no in-image TCP/IP socket layer. That decision
turned the firewall app from a thin filter hook into thousands of
lines of parser-heavy, hash-table-heavy, raw-pointer-adjacent code.

That changes what a good substrate + language pairing looks like:

- **Memory safety pays more rent** in this much code than it did in
  the original thin-hook design.
- **The substrate's value-add changes too.** Unikraft's main
  contribution was its C library catalog (lwIP, musl, uknetdev) —
  most of which we no longer use after ADR 0013. A Rust-native
  unikernel offers more tightly integrated Rust + a maintained
  Rust-native TCP/IP crate (smoltcp) we can pull *only the wire
  layer* of, matching ADR 0013's stance exactly.

## Decision

The thurward substrate stack is:

- **Hermit unikernel** — Rust-native, `no_std`, runs the application
  in kernel space. Targets QEMU/KVM (dev) and Firecracker (production
  VPS / managed host).
- **Rust** — the application language. Edition 2024, `no_std`,
  Cargo-managed. Compile target: `x86_64-unknown-hermit`.
- **smoltcp — wire/parser layer ONLY.** We use `smoltcp::wire`
  (`EthernetFrame`, `Ipv4Packet`, `TcpPacket`, `UdpPacket`,
  `IcmpPacket`) and `smoltcp::storage`-style buffer types as a typed,
  fuzzed, zero-allocation packet-parser library. We do **not** link
  `smoltcp::iface::Interface` or `smoltcp::socket::*`. The
  conntrack table, NAT translation, filter, and TX scheduling all
  stay hand-written in the firewall crate. smoltcp is a library,
  not the stack — same posture ADR 0013 took toward lwIP.

### Why smoltcp at all

- `smoltcp::wire` is fuzzed, slice-based, zero-alloc — matches the
  hot-loop constraints of the fast path
  ([ADR 0013](0013-zig-fast-path-on-uknetdev.md)).
- Encodes wire-format invariants in the type system
  (e.g. `Ipv4Packet::dst_addr()` returns `Ipv4Address`, not `u32`),
  which catches a class of parser bugs at compile time.
- Modular by design: pulling `wire` doesn't drag the socket or
  scheduler layers in.

### Bare-metal: deferred to v2

Hermit's bare-metal direct-boot story is less mature than Unikraft's.
Rather than weaken the substrate decision or weaken the bare-metal
promise, **v1 ships hypervisor targets only** (QEMU/KVM,
Firecracker); bare-metal is a known v2 direction. See
[09 — Limitations](../09-limitations.md). The v2 ADR will reconsider:
Hermit's bare-metal maturity by then; a small Linux + KVM appliance
image as a pragmatic answer; another unikernel's bare-metal story
(Unikraft on ARM, MirageOS, etc.).

## Performance targets

Unchanged from [ADR 0013](0013-zig-fast-path-on-uknetdev.md):
~1 Gbps single-vCPU baseline (v1); 5–10 Gbps with multi-queue + RSS
+ per-thread conntrack shards (v1.x stretch, its own future ADR).
Rust's LLVM codegen produces comparable hot-loop output for this
kind of code; the substrate doesn't move the numbers.

## Consequences

- (+) **Memory safety where it matters.** Parser code (smoltcp + ours),
  conntrack hash table, NAT translation — the surfaces a firewall
  lives or dies on — get Rust ownership/lifetime guarantees.
- (+) **smoltcp's wire types are a real shortcut.** Several hundred
  lines of hand-rolled parser code become a battle-tested crate.
- (+) **Cargo + crates ecosystem.** YAML parsing, schema-validated
  rule loading, the DNS proxy, the observability emitter — every one
  has mature `no_std`-friendly crates available.
- (+) **Hiring / community.** More Rust systems engineers than Zig
  systems engineers; contributor onboarding gets easier.
- (−) **Bare-metal deferred to v2.** The biggest cost of the switch.
  v1 is hypervisor-only.
- (−) **Smaller community than Unikraft.** Hermit is actively
  developed but has fewer downstream users, fewer example projects,
  fewer Stack Overflow answers.
- (−) **Heavier toolchain pinning** than the prior Zig stack:
  `rust-toolchain.toml`, `Cargo.lock`, Hermit revision, smoltcp
  crate version — all in `versions.lock` per the revised
  [ADR 0010](0010-supply-chain-hardening.md).
- (○) Application-side architecture is unchanged from
  [ADR 0013](0013-zig-fast-path-on-uknetdev.md): the app owns the
  data plane; no in-image socket layer.

## Alternatives considered

- **Stay on Unikraft, switch language to Rust.** Doable via
  Unikraft's `lib-rust`; would keep the bare-metal v1 story alive.
  Rejected because `lib-rust` on Unikraft is a thinner integration
  than Hermit's native Rust path, smoltcp integration is bolt-on,
  and Unikraft's main value-add (C library catalog) is mostly libs
  we no longer use after ADR 0013.
- **Stay on Hermit, keep Zig.** Hermit has no first-class Zig story;
  would mean fighting the framework. Rejected.
- **Drop unikernel, use Linux + Rust + nftables/eBPF.** Honest
  answer for the firewall problem class; explicitly off the table
  per the unikernel-minimalism stance ADR 0017 inherits from
  [ADR 0001](0001-unikraft-as-os-substrate.md). Rejected.
- **MirageOS** (with Rust bindings via FFI). OCaml ecosystem, more
  mature unikernel, has the qubes-mirage-firewall production
  lineage. Rejected because Rust ↔ OCaml interop is rough and
  we'd lose smoltcp.

## Relation to other ADRs

- [ADR 0001](0001-unikraft-as-os-substrate.md) — superseded as the
  substrate. Its rejected-alternatives analysis (why a unikernel at
  all) still applies.
- [ADR 0002](0002-zig-as-application-language.md) — superseded as
  the language.
- [ADR 0003](0003-inline-middlebox-topology.md) — unchanged. Two
  virtio NICs; LAN-ingress / WAN-ingress identified by source
  interface.
- [ADR 0005](0005-build-time-rule-compilation.md) — `build.rs`
  (Cargo build script) emits a generated Rust module under
  `$OUT_DIR`.
- [ADR 0010](0010-supply-chain-hardening.md) — pins
  `rust-toolchain.toml` + `Cargo.lock` + Hermit revision + smoltcp
  crate version.
- [ADR 0013](0013-zig-fast-path-on-uknetdev.md) — the
  app-owns-the-data-plane stance lands here in Rust + smoltcp-wire
  vocabulary.
- [ADR 0014](0014-stateful-nat.md) — NAT implementation language is
  Rust; semantics unchanged.
- [ADR 0016](0016-image-per-change-user-deploy.md) — `make build`
  wraps `cargo build --release --target x86_64-unknown-hermit`.
