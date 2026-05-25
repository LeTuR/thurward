# ADR 0018 — Hermit raw NIC access strategy

**Status:** Proposed
**Date:** 2026-05-25
**Deciders:** magicletur

## Context

[ADR 0017](0017-hermit-rust-substrate.md) committed the substrate to
Hermit + Rust + `smoltcp::wire`, with the application owning the
entire data path (per [ADR 0013](0013-zig-fast-path-on-uknetdev.md)
and [02 — Packet path](../02-packet-path.md)). The implementation
plan for phase 1 (`make smoke CAND=thurward`) needs the application
to **receive raw Ethernet frames from one virtio-net device and
re-transmit them on the other** — no TCP/UDP socket layer involved.

During phase-1 PR B research a blocker surfaced: **`hermit-os/kernel`
does not expose a public API for raw frame access.** Every relevant
type is `pub(crate)`:

- The `NetworkDriver` trait (which extends `smoltcp::phy::Device` and
  would be exactly what we need) is declared `pub(crate)` in
  `kernel/src/drivers/net/mod.rs`.
- All concrete drivers (`virtio`, `rtl8139`, `gem`, `loopback`) and
  the `NetworkDevice` type alias are crate-private.
- The single `smoltcp::iface::Interface` instance lives behind a
  `pub(crate) static NIC: InterruptTicketMutex<NetworkState<'_>>` in
  `kernel/src/executor/network.rs`.

The application-facing API in `hermit-abi` is strictly POSIX sockets
(`socket`/`bind`/`listen`/`accept`/`send`/`recv`/`sendto`/`recvfrom`).
There is no `AF_PACKET`, no `SOCK_RAW`, no FFI hatch into the kernel's
smoltcp `Interface`.

So ADR 0017's "smoltcp wire layer only, application owns the data
path" is not satisfiable against upstream `hermit-os/kernel`
unchanged.

This ADR settles **how** we resolve that gap.

## Decision

**Fork `hermit-os/kernel`** at the v0.13.2 tag into
`thurward/hermit-kernel`, apply a minimal patch that exports
`NetworkDriver` and provides a `pub fn take_nics() -> Vec<Box<dyn
NetworkDriver>>` that disables the kernel's smoltcp `Interface`
instantiation and hands raw devices to the application. Pin the
fork via `[patch.crates-io]` in `Cargo.toml` and a vendored git
reference in `versions.lock` per [ADR 0010](0010-supply-chain-hardening.md).

Track upstream via a discussion issue on `hermit-os/kernel`
requesting the same exports under a `raw-net` cargo feature. If the
upstream lands a public API, the fork goes away and the
`[patch.crates-io]` line is removed.

## Consequences

- (+) Preserves every reason ADR 0017 was made: Rust-native, single
  language across the project, modern toolchain, `smoltcp::wire`
  available as a parser library, kernel-space execution model (no
  syscall overhead in the hot loop), and the QEMU/KVM + Firecracker
  deployment targets.
- (+) Patch surface is tiny — three `pub(crate)` → `pub` flips and
  one new accessor function. The agent's source-tree audit suggests
  ~50 lines of delta total. Easy to rebase against upstream.
- (+) Doesn't fork the Rust toolchain or any other crate; the fork is
  scoped to one crate (`hermit`) at one pin.
- (+) Leaves the door open for upstreaming. The patch is exactly the
  shape upstream would accept if they decided to expose raw-frame
  access (under a feature flag).
- (−) Adds maintenance: every Hermit point release needs a rebase
  pass. Mitigated by pinning a known-good revision in
  `versions.lock`; supply-chain hardening ([ADR 0010](0010-supply-chain-hardening.md))
  already requires explicit pins, so this is incremental, not novel.
- (−) Couples the project to upstream's evolution speed. If
  `hermit-os/kernel` reworks its driver layout in a major release,
  the patch may need substantive porting. v1's narrow data path
  contains the blast radius — we don't touch the rest of the kernel.
- (○) A new `versions.lock` field for `hermit_kernel_fork_revision`
  pinning our fork's commit SHA, and a `forks/` directory at the
  repo root holding the patch series. Not yet present; PR B/C will
  add them as the fork goes in.

## Alternatives considered

### A. Revert to Unikraft (ADR 0001 style, but keep Rust)

Unikraft exposes raw frame access through `uknetdev` — that was the
whole point of [ADR 0013](0013-zig-fast-path-on-uknetdev.md)'s
original Unikraft-based fast path. We could keep Rust as the
application language but swap the substrate back.

Rejected because (a) Unikraft's value-add over a thin Rust unikernel
is its C library catalog (lwIP, musl, edk2-ovmf, hundreds of glue
crates) — which we do not consume; (b) we'd lose the
single-toolchain story ADR 0017 valued (Unikraft requires a C
toolchain alongside Rust); (c) Unikraft's
Rust support is still experimental and we'd be the early adopter
rather than upstream-stable user; (d) the ADR 0017 reasoning about
memory safety in a parser-heavy application still applies — we
should stay in Rust.

### B. Custom no_std + Rust + virtio-pmd

Write our own minimal kernel: bootloader, virtio-net driver, a tiny
allocator, a serial console for logs. Stay in Rust, no upstream
substrate dependency at all.

Rejected because the scope is months of work for table-stakes
infrastructure that Hermit already gives us. We'd reinvent
maintenance burden we explicitly outsourced in ADR 0017.

### C. Submit upstream PR and wait

Open a discussion on `hermit-os/kernel` asking for a `raw-net` cargo
feature that exports `NetworkDriver` + a `take_nics()` accessor.
Don't ship thurward's data path until the feature lands upstream.

Rejected because (a) it puts thurward's roadmap on the upstream
maintainer's schedule; (b) the upstream may reasonably reject
"hand the user my whole networking stack" as out of scope;
(c) phase-1's `make smoke CAND=thurward` target slips indefinitely.
**Will be pursued in parallel** as a discussion + courtesy patch, so
if upstream lands the feature we can drop our fork. Not the
critical path.

### D. Hermit + std::net sockets (give up raw access)

The most expedient option: forget raw frames, use Hermit's
`std::net::*` as if thurward were just another application. A LAN
socket forwarding to a WAN socket emulates routing for some traffic.

Rejected. A firewall is fundamentally a forwarding device — packets
arrive on one interface and must be re-emitted on another, with the
source IP and L4 ports of the *original* sender preserved (mod
optional SNAT/DNAT). The socket layer terminates flows at thurward;
it cannot pass them through. This violates every section of
[02 — Packet path](../02-packet-path.md) and breaks the bench smoke
test's `iperf3 gen-LAN → gen-WAN-netns` assertion. Not viable for
the use case.

### E. Linux userspace forwarder (AF_PACKET / DPDK)

Run thurward as a userspace Rust binary on a stripped-down Linux
VM, using `AF_PACKET` (or DPDK in a follow-up) for raw frames.
Phase-1 smoke would pass in days, not weeks.

Rejected. This violates ADR 0017's substrate decision wholesale and
defeats the project's "unikernel firewall" thesis. The whole point
of the architecture is to ship a minimal, signed, build-time-
configured image — not to ship "another Linux VM with a Rust
process". If the team decided unikernel was no longer the goal,
we'd write that as a separate ADR superseding 0017; this ADR is
about resolving a Hermit-specific blocker, not relitigating the
substrate.

## Open question for the deciders

The recommendation above is the fork path (A). Before this ADR moves
from **Proposed** to **Accepted**, confirm:

1. Acceptable to add a `forks/hermit-kernel/` subtree + a
   `[patch.crates-io]` line pinning a thurward-controlled fork?
2. Comfortable with the rebase-against-upstream cost? (Estimated:
   half a day per Hermit release; Hermit's release cadence is
   roughly monthly.)
3. Should the upstream PR (option C) block thurward's PR B/C, or
   run in parallel as a courtesy?

Once accepted, this ADR triggers a follow-up edit on
[ADR 0017](0017-hermit-rust-substrate.md) — its "Consequences"
section gains a pointer here, and ADR 0010's supply-chain section
documents the new `versions.lock` field.
