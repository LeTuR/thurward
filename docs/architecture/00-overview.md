# 00 — Overview

*Audience: anyone who hasn't seen thurward before. Read this first; then
the [decision records](decisions/) in order; then the topical chapters.*

## What thurward is

A minimalistic open-source firewall packaged as a Hermit unikernel
written in Rust. It sits **inline between two networks**, filters
traffic on **source CIDR, destination port, and FQDN**, performs
**SNAT/masquerade and static DNAT**, and is configured by editing
**a single `rules.yaml` in your fork of the firewall repo** and
rebuilding the image. It exposes structured logs, Prometheus metrics,
and per-flow OpenTelemetry traces over virtio-vsock to a host-side
collector — no in-band telemetry traffic. v1 is **single-VM**; HA
and bare-metal direct boot are both deferred to v2.

It is **not** a Linux firewall with extra steps. The OS substrate is
[Hermit](https://hermit-os.org/); the entire data path (parser,
conntrack, filter, NAT, TX) is Rust, using `smoltcp::wire` for typed
packet parsing and writing every other layer ourselves — no in-image
TCP/IP socket layer between the driver and the filter. The image is
a single signed unikernel that boots in milliseconds and exposes no
shell, no mgmt port, no userspace utilities. See
[ADR 0017](decisions/0017-hermit-rust-substrate.md) and
[ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md).

## Why

Production firewalls are usually one of two things:

1. **A Linux box running nftables/iptables** — battle-tested, observable,
   and carrying tens of millions of lines of kernel + userspace as
   attack surface for what's conceptually a small program.
2. **A vendor appliance** — opaque, expensive, often slow to patch.

thurward is an experiment in a third path: **a small, declarative,
observable, fully open-source firewall on a unikernel**, with the
data path written in Rust and no general-purpose TCP/IP socket layer
between the driver and the filter. The goal is production-grade
minimalism — the smallest defensible attack surface that's still
operationally sane, without giving up NAT or gigabit-class throughput.

## Goals

- **Minimalism.** The image contains the kernel libraries thurward
  needs and nothing else. No shell, no package manager, no userspace
  utilities, no inbound management surface.
- **Declarative configuration.** Rules live in `rules.yaml` alongside
  the firewall source. Changes are git commits; deployments are
  build-the-image-and-install. The firewall exposes no runtime admin
  API. See [ADR 0016](decisions/0016-image-per-change-user-deploy.md).
- **FQDN filtering done honestly.** Filter rules can name DNS names.
  Resolution happens via a DNS proxy that learns IPs from real
  responses and honours their TTL. See [ADR 0004](decisions/0004-dns-proxy-for-fqdn-rules.md).
- **First-class observability.** ECS-aligned JSON logs, Prometheus
  metrics with per-rule labels, per-flow OpenTelemetry traces.
  Everything ships out over virtio-vsock — out-of-band by design.
  See ADRs [0006](decisions/0006-vsock-for-observability-egress.md),
  [0007](decisions/0007-ecs-aligned-log-schema.md),
  [0008](decisions/0008-per-flow-tracing-not-per-packet.md).
- **Reproducible, signed, attested, user-verifiable.** Every image is
  built reproducibly from pinned inputs, signed with `cosign`, and
  ships with SLSA-3 provenance. The user (or their config-management)
  runs `cosign verify` and `slsa-verifier` before installing — there
  is no controller in the loop. See
  [ADR 0010](decisions/0010-supply-chain-hardening.md) and
  [ADR 0016](decisions/0016-image-per-change-user-deploy.md).

## Non-goals

- **NAT beyond SNAT + static DNAT.** Hairpin NAT, ALGs (FTP/SIP/etc.),
  CGNAT-scale port allocation, and NAT64/66 are explicitly out of
  scope. See [ADR 0014](decisions/0014-stateful-nat.md) and
  [09 — Limitations](09-limitations.md).
- **IPv6** in v1. Architected to add later, not implemented first.
- **Hot rule reload.** Every rule change is a new signed image.
  See [09 — Limitations](09-limitations.md) and [ADR 0005](decisions/0005-build-time-rule-compilation.md).
- **10G+ throughput in v1.** Single-vCPU baseline targets gigabit-class.
  Multi-queue + RSS for 5–10 Gbps is a v1.x stretch goal per
  [ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md), not v1.
  If you need 40G line-rate, build on DPDK or XDP/eBPF.
- **Lossless HA.** v1 ships sub-second failover with best-effort state
  replication. See [ADR 0015](decisions/0015-active-passive-ha.md).
- **Bypass-resistant DNS policy.** DoH and hardcoded resolvers escape
  the proxy. Mitigation is operational, not architectural.

## Stack at a glance

```
LAN clients
   │
   │  packets + DNS queries
   ▼
┌──────────────────────────────────────────────────┐
│              thurward unikernel                  │
│  ┌──────────────────────────────────────────┐    │
│  │  Rust application (data plane)           │    │
│  │   ├─ parse: smoltcp::wire types          │    │
│  │   │   (Eth / IPv4 / TCP / UDP / ICMP)    │    │
│  │   ├─ conntrack table                     │    │
│  │   ├─ filter (compiled rule table)        │    │
│  │   ├─ NAT (SNAT pool + static DNAT)       │    │
│  │   ├─ DNS proxy + fqdn_set                │    │
│  │   └─ observability emitter (vsock)       │    │
│  ├──────────────────────────────────────────┤    │
│  │  Hermit virtio-net + virtio-vsock        │    │
│  ├──────────────────────────────────────────┤    │
│  │  Hermit unikernel core                   │    │
│  └──────────────────────────────────────────┘    │
└────────┬──────────────────────────┬──────────────┘
         │ NIC 0 (LAN)              │ NIC 1 (WAN)
         ▼                          ▼
   br-lan bridge              br-wan bridge
         │                          │
         ▼                          ▼
       LAN                     upstream router
```

Note: there is no TCP/IP socket layer in the image. `smoltcp` is
linked only for its `wire` / parser modules; the Rust data plane
owns the forwarding loop directly
([ADR 0013](decisions/0013-zig-fast-path-on-uknetdev.md),
[ADR 0017](decisions/0017-hermit-rust-substrate.md)).

(System context, packet path, deployment topology, and the rule/IaC
flow are documented in chapters 01, 02, 06, and 10 respectively, with
proper diagrams.)

## Glossary

- **Unikernel** — a single-purpose operating system image where the
  application and the kernel libraries are linked into one binary.
  Boots on a hypervisor; no shell, no users, no multi-process model.
- **Hermit** — a Rust-native unikernel framework. Application runs in
  kernel space; `cargo build --target x86_64-unknown-hermit` produces
  the image.
- **smoltcp** — a `no_std` Rust TCP/IP stack. thurward links only its
  `wire` (parser/builder) modules — not its socket or interface layers
  — so the application keeps direct control over the forwarding loop.
- **conntrack** — thurward's in-memory flow-state table keyed by the
  canonical (pre-translation) 5-tuple. Backs both `state: established`
  rule matches and NAT reverse-translation.
- **fqdn_set** — thurward's in-memory map of `(qname → {ip, expiry})`,
  populated by the DNS proxy from real DNS responses.
- **virtio-vsock** — a virtio device giving the guest a socket channel
  to the host hypervisor. Not routed; not network-addressable.
- **ECS** — Elastic Common Schema. A field naming convention for
  structured logs that many SIEM and dashboards understand.
- **SLSA** — Supply-chain Levels for Software Artifacts. A maturity
  model for build-pipeline trust.
- **Blue/green swap** — deploying a new image alongside the old one,
  flipping traffic over, then retiring the old one. Used here to
  apply rule changes without dropping established flows.
