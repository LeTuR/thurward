# ADR 0013 — App-owned fast path, no in-image TCP/IP socket layer

**Status:** Accepted (substrate-vocabulary revision 2026-05-25 under ADR 0017)
**Date:** 2026-05-24
**Revised:** 2026-05-25
**Deciders:** magicletur

> *Filename retains the historical `zig-fast-path-on-uknetdev` slug
> for link stability; the substantive content has been brought into
> line with [ADR 0017](0017-hermit-rust-substrate.md).*

## Context

The original substrate decision ([ADR 0001](0001-unikraft-as-os-substrate.md),
now superseded by [ADR 0017](0017-hermit-rust-substrate.md)) paired
the firewall with a pre-built TCP/IP stack inside the image — first
lwIP, then the question was whether to keep that pattern at all.
Pre-built stacks produced two limitations that became unacceptable:

- They don't ship NAT (so thurward could only be a pure allow/deny
  middlebox). See [ADR 0014](0014-stateful-nat.md).
- Their forwarding paths are not built for line-rate; realistic
  throughput on commodity hardware is well under 1 Gbps.

Both are blocking for the edge-firewall role thurward targets. NAT is
table stakes (SNAT/masquerade + DNAT) and gigabit-class throughput
on a single vCPU is the documented v1 goal.

## Decision

**The application owns the data path.** No pre-built TCP/IP stack
sits between the NIC driver and the filter.

Concretely, under the [ADR 0017](0017-hermit-rust-substrate.md)
substrate (Hermit + Rust):

- **The Rust application drives Hermit's virtio-net devices
  directly.** RX and TX queues on both NICs (LAN, WAN) are owned by
  the app; there is no `smoltcp::iface::Interface` or socket layer
  between Hermit and the filter.
- **smoltcp's `wire` module is the only TCP/IP code we link.** We
  use `smoltcp::wire::{EthernetFrame, Ipv4Packet, TcpPacket,
  UdpPacket, IcmpPacket}` for zero-copy slice-based header parsing
  and packet construction. We do **not** link
  `smoltcp::iface::Interface` or `smoltcp::socket::*`. smoltcp is a
  library here, not a stack.
- **Single RX poll loop per interface.** Runs on a dedicated thread
  (Hermit's threading model). Polls RX descriptors in batches
  (default 64), pulls frame pointers, dispatches into the
  parse/match/translate pipeline, pushes onto the egress TX queue.
- **Conntrack, NAT translation, filter, TX scheduling, and the DNS
  proxy are all hand-written Rust in this crate.** Pre-built stacks
  cover none of these; that's the whole reason this ADR exists.
- **No DPDK, no AF\_XDP, no netmap.** Stays inside Hermit's standard
  device abstraction; image stays small; supply-chain hardening
  ([ADR 0010](0010-supply-chain-hardening.md)) stays tractable.

## Performance targets

- **Baseline (v1):** ~1 Gbps sustained on a single vCPU with
  virtio-net, including 64-byte-frame worst case. Achievable by the
  standard fast-path design choices: zero allocation on the hot loop,
  batched descriptor processing, branch-free common-case parsing,
  linear-scan compiled rule table
  ([03 — Rule model](../03-rule-model.md)), open-addressed conntrack
  hash table sized to ~64k flows.
- **Stretch (v1.x):** 5–10 Gbps with virtio-net multi-queue + RSS,
  one RX poll thread per queue, per-thread shards of the conntrack
  table. If pursued, this gets its own ADR; v1 does not commit to
  it.

These numbers are honest, not aspirational. They depend on the
fast-path discipline above, not on language choice — Rust's LLVM
codegen produces tight hot-loop output for this kind of code.

## Consequences

- (+) **NAT is implementable inside thurward** instead of being
  pushed upstream. See [ADR 0014](0014-stateful-nat.md).
- (+) **Throughput moves from "<1 Gbps" to "gigabit-class on a
  single vCPU"**, with multi-queue as the documented next step.
- (+) **Smaller dependency surface.** No full TCP/IP stack inside
  the image. smoltcp's `wire` modules are a small fraction of its
  total LOC; the socket/interface layers we don't link are several
  thousand lines of code we don't ship.
- (+) **Auditability improves.** The data plane is the program. No
  hidden behaviour from a pre-built stack's state machines.
- (+) **Memory safety where it matters.** Parser code, conntrack
  hash table, NAT translation — exactly the surfaces a firewall
  lives or dies on — get Rust's ownership/lifetime guarantees.
- (−) **More code to write and maintain.** The parser, conntrack,
  NAT translation, and TX scheduler are now ours — they were the
  stack's. Net work, justified by the unblocked features.
- (−) **Edge cases pre-built stacks handle implicitly** (fragment
  reassembly, TCP state-machine quirks, ICMP error generation) we
  now decide on explicitly. v1 keeps the existing "drop fragments,
  no ICMP errors emitted" stance from
  [09 — Limitations](../09-limitations.md); lifting either is a
  separate ADR.
- (−) **No off-the-shelf TCP/IP correctness suite to inherit.**
  We'll need our own fuzzing/conformance tests in the implementation
  phase. smoltcp's `wire` parsers are themselves fuzzed; what we
  need to fuzz is our parse-to-conntrack-to-NAT pipeline.
- (○) **Single-threaded data plane is retained for v1.** Multi-queue
  is an additive change for the v1.x stretch goal, not a redesign.

## Alternatives considered

- **Keep a pre-built TCP/IP stack; add NAT as a shim around its
  forwarding path.** Fixes NAT but not throughput; the README's
  "gigabit-class" claim would still be a lie. Rejected.
- **Adopt DPDK.** Would deliver 10G+ comfortably, but pulls in a
  large dependency stack, hugepages, and a poll-mode driver model
  that fights the minimalism stance from
  [ADR 0001](0001-unikraft-as-os-substrate.md) (which ADR 0017
  preserves). Not v1; revisit only if 10G+ becomes a hard
  requirement.
- **Pivot to Linux + XDP/eBPF.** Best NAT/perf ecosystem in
  existence (nftables conntrack, XDP, AF\_XDP). Rejected for the
  same reasons ADR 0001 originally rejected it: the kernel attack
  surface defeats the minimalism goal, and the user picked the
  unikernel path.
- **Use smoltcp's full stack (iface + sockets) on the data path.**
  Tempting because it would let us drop the hand-written forwarding
  loop. Rejected for the same reason we rejected lwIP: a pre-built
  stack owning the loop is incompatible with NAT-integration and
  with the latency/batching control we need.

## Relation to other ADRs

- [ADR 0001](0001-unikraft-as-os-substrate.md) — superseded by
  ADR 0017; the substrate-discards-pre-built-TCP/IP-stack stance
  this ADR introduced is unaffected by that supersession.
- [ADR 0017](0017-hermit-rust-substrate.md) — current substrate;
  Hermit + Rust + smoltcp wire-only is the implementation vocabulary
  this ADR's design lands in.
- [ADR 0003](0003-inline-middlebox-topology.md) — unchanged. Still
  two virtio NICs; LAN-ingress / WAN-ingress identified by source
  interface.
- [ADR 0014](0014-stateful-nat.md) — enabled by this ADR; defines
  the NAT semantics that the fast path executes.
- [ADR 0015](0015-active-passive-ha.md) — deferred to v2; would
  replicate the conntrack table this ADR defines when v2 HA work
  resumes.
