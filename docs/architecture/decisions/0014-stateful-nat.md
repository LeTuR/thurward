# ADR 0014 — Stateful NAT (SNAT/masquerade + DNAT)

**Status:** Accepted
**Date:** 2026-05-24
**Deciders:** magicletur
**Depends on:** [ADR 0013](0013-zig-fast-path-on-uknetdev.md) — NAT
lives in the app-owned fast path; impossible while a pre-built
TCP/IP stack owns forwarding.
**Substrate:** [ADR 0017](0017-hermit-rust-substrate.md) — Rust
implementation on Hermit.

## Context

thurward is now positioned as an edge firewall (SOHO/branch). At the edge,
NAT is table stakes:

- **SNAT/masquerade** so a LAN sharing one WAN IP can reach the internet.
- **DNAT (port-forward)** so a few internal services can be exposed on the
  WAN IP — a handful of static entries, not arbitrary load-balancing.

Hairpin NAT (LAN → public-IP → back-to-LAN), CGNAT-scale port allocation,
NAT64/NAT66, and ALGs (FTP, SIP, etc.) are explicitly *not* in scope.
ADR 0013 unblocked NAT by removing lwIP from the data path; this ADR
defines what NAT means inside thurward.

## Decision

Implement stateful NAT in the app-owned fast path (Rust per
[ADR 0017](0017-hermit-rust-substrate.md)) with the following shape.

### Conntrack table

- **Key:** `(proto, src_ip, src_port, dst_ip, dst_port)` for TCP/UDP;
  `(proto, src_ip, dst_ip, icmp_id)` for ICMP echo. Stored canonically
  (the pre-translation tuple from the LAN client's perspective).
- **Value:** translation entries (`snat_src_ip`, `snat_src_port`,
  `dnat_dst_ip`, `dnat_dst_port`, each optional), TCP state, last-seen
  timestamp, byte/packet counters.
- **Sizing:** open-addressed hash table, default 64k entries. Capacity
  is a build-time constant — no runtime allocation.
- **Timeouts:** TCP state-aware (SYN_SENT 30s, ESTABLISHED 1h,
  TIME_WAIT 60s; tunable in `rules.yaml` `defaults`), UDP idle 60s,
  ICMP echo 30s. Eviction is lazy on the next bucket-scan, plus a
  periodic sweep on the poll thread when load permits.
- **Not replicated.** v1 ships single-VM with no HA peer
  ([09 — Limitations](../09-limitations.md)); the conntrack table
  is therefore process-local and is lost on restart. The state-sync
  design originally drafted in
  [ADR 0015](0015-active-passive-ha.md) is deferred to v2.

### SNAT / masquerade

- Configured per egress interface in `rules.yaml` (`nat.snat`):

  ```yaml
  nat:
    snat:
      - out_interface: wan0
        source: 10.0.0.0/24
        masquerade: true          # use the interface's primary IP
        # or: external_ip: 203.0.113.10
  ```

- **Port pool** per `(external_ip, proto)`: 16384–60999 by default,
  configurable. Allocation is `(hash(tuple) % pool) + base`, with
  linear probing on collision; deterministic given the same tuple
  ordering at boot.
- **Symmetric**: on egress, allocate (if no conntrack entry) and
  translate `src_ip:src_port`. On return ingress, the conntrack entry's
  reverse lookup translates `dst_ip:dst_port` back to the LAN endpoint.

### DNAT (port-forward)

- Configured statically in `rules.yaml` (`nat.dnat`):

  ```yaml
  nat:
    dnat:
      - wan_ip: 203.0.113.10
        wan_port: 443
        to: 10.0.0.5:443
        proto: tcp
        from: 0.0.0.0/0           # optional source CIDR filter
  ```

- DNAT entries are compiled into the image like filter rules
  ([ADR 0005](0005-build-time-rule-compilation.md)). No runtime
  reconfiguration; rule changes flow through the user-driven build +
  install workflow
  ([ADR 0016](0016-image-per-change-user-deploy.md)).
- On WAN-ingress, the fast path consults the DNAT table *before* the
  filter rule scan. If a DNAT entry matches, the packet is rewritten
  to its internal destination *and* a conntrack entry is installed,
  *and* the filter rule scan runs against the rewritten 5-tuple.

### Filter / NAT ordering

Deliberate, documented:

| Direction       | Step 1                    | Step 2                | Step 3              |
| --------------- | ------------------------- | --------------------- | ------------------- |
| LAN → WAN       | filter (pre-SNAT tuple)   | conntrack lookup/insert | SNAT translate    |
| WAN → LAN (new) | DNAT lookup + translate   | filter (post-DNAT tuple) | conntrack insert |
| WAN → LAN (est.) | conntrack reverse-lookup → de-SNAT or de-DNAT | filter (post-translate) | — |

The principle: **rule authors always write rules against real internal
IPs**, never against translated addresses. The filter sees `10.0.0.5:443`
whether the packet was DNAT'd or originated internally; it never sees
the `203.0.113.10:443` WAN-facing tuple.

## Consequences

- (+) thurward becomes deployable as an edge router replacement
  (SOHO/branch). The README's "inline middlebox between LAN and WAN"
  positioning now matches real-world expectations.
- (+) Conntrack also unlocks the existing `state: established` rule
  semantic without leaning on lwIP's internals — that rule type now
  has a documented backing store.
- (+) DNAT is build-time-compiled, same trust model as filter rules.
  No new runtime admin surface
  ([ADR 0016](0016-image-per-change-user-deploy.md) intact).
- (−) Memory footprint grows by the conntrack table size (64k entries
  × ~64 bytes ≈ 4 MB). Acceptable for a unikernel that was already
  sized at 128 MB in [06 — Deployment](../06-deployment.md).
- (−) NAT semantics are now part of the schema surface; the JSON
  Schema in [ADR 0011](0011-schema-first-iac-no-custom-provider.md)
  grows accordingly.
- (○) `state: established` rule semantics now refer to *thurward's*
  conntrack, not lwIP's. Behaviour is the same; the source-of-truth
  pointer moves.

## Out of scope

- **Hairpin NAT.** A LAN client connecting to its own WAN IP won't
  loop through DNAT correctly. Documented in
  [09 — Limitations](../09-limitations.md).
- **ALGs** (FTP, SIP, H.323, PPTP, IRC DCC, etc.). Protocols that
  embed IP addresses inside payloads stay broken. Same chapter.
- **CGNAT** (deterministic-port-block allocation, subscriber bindings).
  Different problem class; out of scope for an edge firewall.
- **NAT64 / NAT66 / DNS64.** IPv6 itself is still a v1 limitation
  ([09 — Limitations](../09-limitations.md)); translation between
  families is therefore moot for now.
- **Per-flow QoS / rate-limiting via NAT.** Possible future feature;
  not v1.

## Alternatives considered

- **Stateless NAT only (pure rewrite, no conntrack).** Half the code;
  doesn't actually work for TCP/UDP return traffic without an
  out-of-band state store, so this is "NAT in name only". Rejected.
- **Borrow conntrack semantics from `lib-lwip`.** ADR 0013 removed
  lwIP from the data path; this would re-add it just for state
  tracking, which defeats the point.
- **Runtime DNAT updates via an admin channel.** Tempting for
  port-forward agility; conflicts with
  [ADR 0016](0016-image-per-change-user-deploy.md) and the threat
  model. Rejected.

## Relation to other ADRs

- [ADR 0005](0005-build-time-rule-compilation.md) — extended: DNAT
  entries compile into the image alongside filter rules.
- [ADR 0016](0016-image-per-change-user-deploy.md) — preserved: no
  runtime NAT admin surface.
- [ADR 0011](0011-schema-first-iac-no-custom-provider.md) — extended:
  JSON Schema gains `nat.snat[]` and `nat.dnat[]` definitions.
- [ADR 0013](0013-zig-fast-path-on-uknetdev.md) — depended on; NAT
  lives in the fast path it defines.
- [ADR 0015](0015-active-passive-ha.md) — deferred to v2; would
  replicate this conntrack table when v2 HA work resumes.
