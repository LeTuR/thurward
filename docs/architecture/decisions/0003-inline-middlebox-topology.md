# ADR 0003 — Inline middlebox (two-NIC) topology

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

A firewall can sit at three places in a network:

1. **Host-local** — filters only traffic to/from the host running it
   (`ufw`, `firewalld`). Cannot protect *other* machines.
2. **Inline middlebox** — has two network interfaces, forwards or drops
   traffic between them. Protects everything on the LAN side.
3. **Workload sidecar** — sits next to a single application, filters
   only that application's traffic (Cilium-style egress).

The user explicitly chose the inline middlebox topology. This rules out
"thurward as a host firewall" entirely.

## Decision

thurward runs as a unikernel VM with **two** virtio network interfaces.
One face is the LAN (clients live here, use thurward as their default
gateway and DNS resolver); the other face is the WAN (upstream router /
the rest of the world). The Rust fast path
([ADR 0013](0013-zig-fast-path-on-uknetdev.md),
[ADR 0017](0017-hermit-rust-substrate.md)) owns both interfaces directly
through Hermit's virtio-net device API: packets enter one NIC, traverse
parse → filter → NAT → TX, and exit the other. HA returns in v2
([ADR 0015](0015-active-passive-ha.md)); v1 ships one VM at a time.

## Consequences

- (+) Protects every host on the LAN, not just one.
- (+) DNS-proxy interception ([ADR 0004](0004-dns-proxy-for-fqdn-rules.md))
  becomes natural: thurward already terminates DNS on the LAN side
  because it *is* the resolver, no transparent redirect needed.
- (+) Egress FQDN filtering is the same code path as ingress port
  filtering — one filter, two directions, identified by source interface.
- (−) Two TAP devices and two Linux bridges on the host (`br-lan` /
  `br-wan`) — more network plumbing than a host firewall. With HA
  ([ADR 0015](0015-active-passive-ha.md)) this becomes two VMs sharing
  the same two bridges; the bridge plumbing itself doesn't grow.
- (○) The LAN side terminates DNS, DHCP (optional, future), and IP
  forwarding; the WAN side is a pure forwarding interface with no
  client-facing services.

## Alternatives considered

- **Host-local firewall.** Simpler topology (one NIC, no forwarding),
  but doesn't protect anything except the host. Rejected: defeats the
  point of building a standalone firewall appliance.
- **Workload sidecar.** Cleanest scope (filter one app's traffic),
  best FQDN-policy fit (you know the app's identity). Rejected because
  the user wants a general-purpose firewall, not a per-app component.
- **Single-NIC transparent bridge.** Possible (Linux bridge with
  `ebtables`), but the two-NIC model is easier to reason about and
  observe, and aligns cleanly with the LAN-ingress / WAN-ingress
  direction axis used throughout the rule model.
