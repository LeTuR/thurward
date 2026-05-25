# ADR 0008 — Per-flow tracing, not per-packet

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

OpenTelemetry traces give operators a per-request narrative: how long
DNS lookup took, which rule matched, where time went. The natural
question for a firewall: trace what — every packet, or something
coarser?

A packet-rate trace span is infeasible. Even at modest line rates
(say, 100k packets/second), per-packet spans would generate millions
of spans per second per firewall, dominating the vsock channel and
making the collector pipeline meaningless to operators. The signal
has to be coarser.

The natural coarsening is **per-flow**: emit one span per new 5-tuple
(or per first SYN for TCP, per first datagram for UDP) summarising the
decision and the relevant latency components (DNS lookup if any, rule
match time, fqdn_set check).

## Decision

Emit one OTel span per new flow at flow-creation time. Flows are
identified by the 5-tuple `(src_ip, src_port, dst_ip, dst_port, proto)`.
For TCP, "flow creation" = first SYN observed. For UDP, "flow creation"
= first datagram with a new 5-tuple. Track active flows in a bounded
hash map; evict on RST/FIN (TCP) or idle timeout (UDP, default 5 min).

Span attributes:

- `network.transport`
- `source.ip` / `source.port`
- `destination.ip` / `destination.port`
- `destination.domain` (if known)
- `rule.id` (which rule decided this flow)
- `event.action` (`accept` / `drop`)
- `firewall.dns_lookup_ms` (if the flow required a DNS lookup that
  hit the proxy in the recent past — looked up via the proxy's
  own span context)

Sample at 100% for dropped flows, 1% for accepted flows (configurable
in `rules.yaml` global section). Sampling decision is local — no
distributed trace propagation through the firewall (we're a middlebox,
not a participant).

## Consequences

- (+) Trace volume is bounded by *new flow rate*, not packet rate.
  Realistic at line speed.
- (+) The signal operators care about — "why did this connection get
  dropped" — is exactly what a per-flow trace shows.
- (+) DNS proxy spans link naturally with flow spans via parent/child
  relationship within thurward's own trace context.
- (+) Dropped flows are 100%-sampled so investigating outages doesn't
  hit a sampling cliff.
- (−) Per-packet pathologies (intermittent drops mid-flow, MTU issues,
  fragmentation events) are *not* covered by trace spans. They land in
  ECS log events instead (see [ADR 0007](0007-ecs-aligned-log-schema.md))
  and per-rule metrics.
- (−) Flow tracking adds memory cost proportional to active flow count.
  Bounded by a hash-map cap (e.g. 64k flows); eviction at cap drops
  trace coverage for the noisiest sources first.
- (○) Sampling is local; we don't propagate `traceparent` headers
  (we don't terminate L7, and injecting headers would break transparency).

## Alternatives considered

- **Per-packet spans.** Infeasible at any non-trivial line rate. Even
  if we could ship them, no operator would read them.
- **No traces at all, metrics + logs only.** Cheaper. Rejected because
  the user explicitly asked for "clear metrics, traces and logs" —
  flow-level traces are the right granularity for "traces" here.
- **Sampled per-packet (e.g. 1-in-N packets).** Random samples don't
  reconstruct flows; debugging value is much lower than per-flow.
- **eBPF/perf-style aggregated traces.** No equivalent inside a
  unikernel substrate (Hermit or otherwise); outside the constraints
  of the platform.
