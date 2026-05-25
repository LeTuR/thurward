# ADR 0006 — virtio-vsock for observability egress

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

thurward emits a lot of observability data: per-rule counters, per-flow
trace spans, per-packet drop events, DNS resolution events. That data
has to leave the unikernel and reach a host-side collector (OTel
Collector → Prometheus + Loki + Tempo + Grafana — see
[01 — System context](../01-system-context.md)). The candidate channels:

1. **Serial console** (UART/PL011). Universal, simple, but the throughput
   ceiling is roughly 115200 bps — fine for boot messages, useless for
   per-packet logs.
2. **Network egress** over one of the data NICs or a third mgmt NIC.
   High throughput, but observability traffic competes for bandwidth
   with the data plane it's observing, and adds an "is the firewall
   talking to its collector?" rule to the very firewall it's observing.
3. **virtio-vsock**. A virtio device that gives the guest a socket-like
   channel to the host hypervisor process, identified by (CID, port).
   Designed exactly for guest↔host control-channel traffic. No IP, no
   routing, no firewalling.

## Decision

Open a single virtio-vsock stream (host CID `2`, port configurable, default `9000`)
at boot. Multiplex log lines and metric updates over the stream by
content-type prefix:

- Lines starting with `{` → ECS-aligned JSON event (see [ADR 0007](0007-ecs-aligned-log-schema.md)).
- Lines starting with `# METRIC ` → Prometheus text-format metric update.
- Lines starting with `# SPAN ` → OTel span (see [ADR 0008](0008-per-flow-tracing-not-per-packet.md)).

On the host, `socat VSOCK-LISTEN:9000,reuseaddr,fork - | <collector>`
bridges the stream into the OTel Collector, which splits by prefix and
routes to Prometheus / Loki / Tempo.

## Consequences

- (+) Observability traffic never traverses the data NICs — no
  bandwidth contention, no self-referential firewall rule, no
  observability traffic visible to a network attacker.
- (+) No inbound TCP/UDP ports for telemetry — `vsock` is unrouteable
  from outside the host. Zero new network attack surface.
- (+) Throughput is plenty (gigabits) — supports per-flow tracing
  without backpressure under normal load.
- (+) Boot-time setup is dead simple: open one socket, write text lines.
  No TLS, no auth, no service discovery.
- (−) Requires the `vhost_vsock` kernel module on the host. Documented
  in [09 — Limitations](../09-limitations.md).
- (−) Single-line text protocol means parsing on the host side. The
  OTel Collector handles this with the `filelog` receiver + regex
  router; no bespoke code needed.
- (−) Per-CID identity: if multiple thurward VMs run on one host they
  each need a distinct host-side port (CIDs are unique per VM).
- (○) `fqdn_set_warm` events (see [07 — Operations](../07-operations.md))
  can be replayed on next boot via a host-side recorder — useful
  optional feature, not required for v1.

## Alternatives considered

- **Serial console.** Throughput is too low; flagged above.
- **Egress over the WAN NIC.** Adds attack surface and self-referential
  firewall rules. Rejected.
- **Dedicated mgmt NIC.** Solves the bandwidth issue but adds an
  inbound interface to the unikernel — the very thing we excluded
  in [ADR 0016](0016-image-per-change-user-deploy.md) (and earlier
  in the superseded [ADR 0009](0009-gitops-rule-control-plane.md)
  whose threat-comparison table still applies).
- **virtio-fs + structured log files.** Works but inverts the
  push/pull direction (host has to read files), adds filesystem code
  to the unikernel, and complicates rotation. vsock streaming is
  simpler.
