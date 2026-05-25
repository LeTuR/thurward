# ADR 0018 — Substrate pivot: Unikraft (with Rust)

**Status:** Accepted
**Date:** 2026-05-25
**Deciders:** magicletur
**Supersedes:** [ADR 0017](0017-hermit-rust-substrate.md) (substrate clause
only — language clause "Rust" and parser clause "smoltcp::wire" stay
intact)
**Restores:** [ADR 0001](0001-unikraft-as-os-substrate.md) (substrate)

## Context

[ADR 0017](0017-hermit-rust-substrate.md) chose Hermit over Unikraft
with two reasons:

1. Rust-native — single toolchain across the project (versus
   Unikraft's primarily-C ecosystem).
2. Unikraft's value-add (its C library catalog: lwIP, musl,
   uknetdev) was "mostly libs we no longer use after ADR 0013".

Phase-1 PR B research surfaced two problems with that reasoning.

**Reason 1 still holds, narrowly.** Hermit IS Rust-native. But its
runtime model assumes the application *consumes* the network stack
via `std::net::*`. Every layer below sockets is `pub(crate)`:
`NetworkDriver` trait, concrete virtio driver, the `NIC` static
holding the smoltcp `Interface` instance. The application-facing
`hermit-abi` has no `AF_PACKET`, no `SOCK_RAW`, no FFI hatch.

A firewall is fundamentally a forwarding device: packets arrive on
one interface and must be re-emitted on another, preserving the
original sender's IP and L4 ports. The socket layer *terminates*
flows; it cannot pass them through. So Hermit's stock API can't run
thurward's data path, and a fork is the only path forward — patching
upstream against its grain.

**Reason 2 was wrong.** We DO want one specific piece of Unikraft's
value-add: **uknetdev**, the raw-frame device API. That's *exactly*
the surface ADR 0013 / chapter 02 specify. Unikraft was designed
with middlebox use cases in mind; Hermit was not.

So the original ADR 0017 reasoning that swung the decision Hermit-ward
doesn't survive contact with the implementation.

## Decision

Pivot the substrate back to **Unikraft**. Keep Rust as the
application language ([ADR 0017](0017-hermit-rust-substrate.md)'s
language clause is preserved). Keep `smoltcp::wire` as the parser
library (it's just a `no_std` crate — its usefulness is independent
of the surrounding kernel).

Concretely, the v1 substrate stack becomes:

- **Unikraft** — `lib-uknetdev` for raw frame TX/RX from the
  application; `lib-ukboot` for boot; `lib-ukalloc` for the heap;
  `lib-uksched` for cooperative threads; `lib-ukbus-virtio` for the
  PCI bus driver. **Not** linked: `lib-lwip`, `lib-musl`,
  `lib-tlsf`'s socket layer (we own the network stack per
  [ADR 0013](0013-zig-fast-path-on-uknetdev.md)).
- **Rust** — application language via Unikraft's `lib-rust`
  Rust-on-Unikraft integration. Edition 2024, `no_std`, Cargo-
  managed. Build target: a `Kraftfile`-driven cross-compile that
  produces a Unikraft image with our Rust crate as the application.
- **smoltcp** — wire layer only, exactly as ADR 0017 specified.
  Independent of the surrounding substrate.
- **Targets** — QEMU/KVM (dev), Firecracker (production). Bare-metal
  remains out of scope for v1, matching
  [ADR 0017](0017-hermit-rust-substrate.md)'s position; the
  substrate switch doesn't change that.

The fork-hermit-kernel path that the *previous* draft of ADR 0018
(see closed PR #4) proposed is rejected: a fork is a band-aid for a
substrate mismatch, not the right primitive.

## Consequences

- (+) **Native raw NIC access.** `lib-uknetdev`'s public API gives
  us exactly what [02 — Packet path](../02-packet-path.md) specified
  (per-interface batched RX/TX, no allocator on the hot loop). No
  patch, no fork, no upstream-PR wait.
- (+) **Used by middleboxes in production.** Unikraft is the
  substrate a number of network appliances ship on. We're a normal
  user of the project, not a fork-maintaining outlier.
- (+) **Rust ↔ smoltcp::wire ↔ NAT/conntrack stays unchanged.**
  Everything above the substrate layer — the rule compiler from
  PR #3, the dataplane planned in PR C, the conntrack and NAT
  planned in phase 2 — is substrate-agnostic Rust.
- (−) **`lib-rust` is less mature than Hermit's native Rust
  support.** Rust crates that assume a hosted runtime
  (`std::*::*`) may not build; we'd need to gate on `no_std`. The
  hot loop is already `no_std`-friendly per ADR 0013; the open
  question is the build-time crates (serde_yml is `std` — but
  build.rs runs on host, not in the image, so that's fine).
- (−) **C build system to operate.** `make ARCH=x86_64
  PLAT=kvm defconfig` and the rest of the Unikraft build flow.
  Adds tooling the project didn't otherwise need. Mitigation:
  hide it behind one `make build` target in our root Makefile.
- (−) **Bigger build matrix.** We pick a Unikraft revision pin, a
  rust toolchain pin, AND a set of `lib-*` versions. `versions.lock`
  grows. Supply-chain hardening per
  [ADR 0010](0010-supply-chain-hardening.md) was already going to
  pin those — incremental, not novel.
- (○) **Image footprint is larger than Hermit's.** Unikraft includes
  more glue than a Rust-native unikernel. Still well under any
  Linux distro; not a problem against ADR 0017's minimalism stance.

## Alternatives considered

### A. Fork `hermit-os/kernel`

Cited in the closed PR #4 draft. Rejected because the patch is
working against the substrate's design intent — Hermit's model is
"app uses network stack", ours is "app IS network stack". Even with
a small ~50-line delta, every Hermit release becomes a rebase
liability for a mismatch that won't go away.

### B. Distroless Linux + Rust + `AF_PACKET` (or XDP/eBPF)

Practical, well-understood path. nftables / Cilium / Cloudflare's
production firewalls live here. Rejected because it abandons the
project's "unikernel firewall" thesis. The minimalism story would
become "cosign-signed distroless OCI image" — a different
architecture entirely. If we ever decide the unikernel framing is
the wrong target, this is the alternative to consider, but that's
an ADR that supersedes ADR 0001 in spirit, not the right reaction
to a Hermit-specific blocker.

### C. Wait for upstream Hermit to expose raw API

Rejected. Even if upstream eventually exposes a `RawDevice` trait,
we'd still be using Hermit against its design intent (an app that
consumes the stack, not an appliance that *is* the stack). The
substrate-shape mismatch survives the API gap. We are not opening
an upstream courtesy issue.

### D. Custom no_std + virtio-pmd from scratch

Rejected — months of work to reinvent what Unikraft already does
(boot, virtio bus enumeration, IRQs, scheduler, allocator).

### E. MirageOS

OCaml-based. Has the qubes-mirage-firewall lineage. Rejected for the
same reason ADR 0017's analysis rejected it: Rust ↔ OCaml interop
is rough and we'd lose smoltcp.

## Recorded decisions

The three confirmation questions raised when this ADR was Proposed
have been answered:

1. **Substrate pivot to Unikraft: accepted.** `lib-rust` maturity is
   a real cost, mitigated by pinning a known-good Unikraft revision
   in `versions.lock` per
   [ADR 0010](0010-supply-chain-hardening.md). The first
   Unikraft + Rust build (phase-1 PR B) is the gating signal; if it
   reveals a hard `lib-rust` blocker, the deciders revisit then.
2. **Bare-metal stays out of scope for v1.** Matches
   [ADR 0017](0017-hermit-rust-substrate.md)'s position; the
   substrate switch doesn't relax that.
3. **No upstream courtesy issue.** See alternative C above.

## Migration

These edits are mechanical and land in a single follow-up PR
immediately after this ADR is merged:

1. Edit [ADR 0017](0017-hermit-rust-substrate.md): change Status to
   `Accepted (substrate clause superseded by ADR 0018)`. The
   language + smoltcp::wire clauses stay live; the Hermit-specific
   reasoning gets annotated, not removed (history should be
   readable).
2. Update [ADR 0001](0001-unikraft-as-os-substrate.md): change
   Status from `Superseded by ADR 0017` to `Substrate clause
   restored by ADR 0018; language clause superseded by ADR 0017`.
3. Update chapter docs that name "Hermit" specifically:
   `02-packet-path.md` (the Hermit RX poll mention),
   `06-deployment.md` (toolchain section), `09-limitations.md`
   (bare-metal note). Either soften to "the substrate" or rewrite
   to reference Unikraft's `lib-uknetdev`.
4. Update [ADR 0010](0010-supply-chain-hardening.md)'s supply-chain
   section to add Unikraft revision + `lib-*` pin fields to
   `versions.lock`.
5. Update the in-progress `versions.lock` (currently lists Hermit
   TBDs from PR #3) to point at Unikraft instead.
