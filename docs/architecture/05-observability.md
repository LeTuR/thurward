# 05 — Observability

*Audience: anyone setting up dashboards, alerts, or trying to read
thurward's output.*

The decisions that shape this chapter:

- [ADR 0006](decisions/0006-vsock-for-observability-egress.md) —
  observability ships out over virtio-vsock, never over the data NICs.
- [ADR 0007](decisions/0007-ecs-aligned-log-schema.md) — drop/accept
  events use Elastic Common Schema fields.
- [ADR 0008](decisions/0008-per-flow-tracing-not-per-packet.md) — one
  OTel span per flow, not per packet.

## What thurward emits

Three signal types, all multiplexed onto a single vsock stream by
content-type prefix:

| Prefix         | Meaning                              | Destination              |
| -------------- | ------------------------------------ | ------------------------ |
| `{`            | ECS-aligned JSON drop/accept event   | Loki                     |
| `# METRIC `    | Prometheus text-format metric update | Prometheus               |
| `# SPAN `      | OTel span (JSON serialised)          | Tempo (or Jaeger)        |

The host-side `socat VSOCK-LISTEN:9000,fork` feeds the stream into
the OpenTelemetry Collector, which routes by prefix.

## Log schema (ECS)

Every drop or accept event emits one JSON line with these fields
exactly (full list in [ADR 0007](decisions/0007-ecs-aligned-log-schema.md)):

```json
{
  "@timestamp": "2026-05-23T18:42:11.123Z",
  "event.kind": "event",
  "event.category": "network",
  "event.action": "drop",
  "event.outcome": "failure",
  "rule.id": "default-deny",
  "rule.name": "Default deny",
  "source.ip": "10.0.0.42",
  "source.port": 54231,
  "destination.ip": "140.82.112.4",
  "destination.port": 22,
  "destination.domain": "github.com",
  "network.transport": "tcp",
  "network.direction": "egress",
  "network.translated.address": "203.0.113.10",
  "network.translated.port": 54231,
  "firewall.drop_reason": "no_matching_allow_rule"
}
```

`destination.domain` is populated when `fqdn_set` has a binding for
`destination.ip` — operators see *why* an allow happened, not just
that it happened.

`network.translated.address` / `network.translated.port` are populated
on NAT-touched flows (post-SNAT egress, post-DNAT ingress) so a flow
can be correlated across the LAN/WAN boundary.

## Metrics (Prometheus)

Per-rule counters (one of each, per rule id):

- `thurward_rule_accepts_total{rule_id="..."}`
- `thurward_rule_drops_total{rule_id="..."}`

Per-drop-reason histogram-like counter:

- `thurward_drops_total{drop_reason="..."}`
  with values: `no_matching_allow_rule`, `explicit_deny`,
  `fragmented_ipv4`, `ipv6_unsupported`, `malformed_packet`,
  `rate_limit_dns`, `conntrack_full`, `nat_port_pool_exhausted`,
  `dnat_no_backend`.

Conntrack + NAT (see [ADR 0014](decisions/0014-stateful-nat.md)):

- `thurward_conntrack_entries` — gauge.
- `thurward_conntrack_inserts_total` / `thurward_conntrack_evictions_total{reason="..."}`.
- `thurward_nat_port_pool_in_use{external_ip="...",proto="..."}` — gauge.
- `thurward_nat_translations_total{kind="snat|dnat"}` — counter.

HA metrics are deferred to v2 along with
[ADR 0015](decisions/0015-active-passive-ha.md). v1 ships single-VM /
single-box ([09 — Limitations](09-limitations.md)); when HA returns,
the `thurward_ha_*` gauges and counters drafted in 0015 come back too.

DNS proxy:

- `thurward_dns_queries_total{outcome="..."}` — `nxdomain`, `servfail`,
  `success`, `truncated_upgraded_to_tcp`, `rate_limited`.
- `thurward_dns_lookup_duration_seconds` — histogram, p50/p95/p99.
- `thurward_fqdn_set_size` — gauge.
- `thurward_fqdn_set_evictions_total{reason="..."}` — `ttl_expired`,
  `capacity_pressure`.

Vsock backpressure (so operators see when the channel is the bottleneck):

- `thurward_vsock_send_queue_depth` — gauge.
- `thurward_vsock_dropped_lines_total` — counter (lines dropped because
  the channel was full; if non-zero, the dashboard is incomplete).

Build identity (so operators can pin observed behaviour to a binary):

- `thurward_build_info{version="...",git_sha="...",rules_sha="..."}` — gauge=1.

## Traces (OpenTelemetry)

One span per new flow, emitted at flow-creation time. Attributes:

- `network.transport`, `source.ip`, `source.port`,
  `destination.ip`, `destination.port`, `destination.domain`,
  `rule.id`, `event.action`, `firewall.dns_lookup_ms`.

Sampling: 100% for dropped flows, 1% for accepted flows (defaults;
configurable in `rules.yaml` global section per
[03 — Rule model](03-rule-model.md)).

DNS proxy spans link to flow spans through parent/child relationships
within thurward's own trace context — we don't propagate `traceparent`
to clients (we're a middlebox, not an L7 participant).

## Vsock multiplexing protocol

The protocol is deliberately trivial:

```
{...one ECS JSON event per line...}\n
# METRIC thurward_rule_drops_total{rule_id="default-deny"} 12034\n
# SPAN {"trace_id":"...","span_id":"...","attrs":{...}}\n
```

Each line is one observation. No framing, no length prefix — the
collector treats it as a `filelog` source with a regex router that
dispatches by the first character (`{`) or first 8 characters (`# METRIC`,
`# SPAN`).

If the vsock send queue fills (host collector is down or slow), thurward
drops the *oldest* unsent metric/log/span lines and increments
`thurward_vsock_dropped_lines_total`. Drops are recorded; the data
plane is never blocked on observability.

## Recommended dashboards

A Grafana dashboard `Firewall overview` should include:

- Top-of-screen: target health (Prometheus target up/down).
- Time series: pkts/sec per rule (`thurward_rule_accepts_total` rate),
  drops/sec per drop reason.
- Top-N table: most-active source IPs, most-dropped destination FQDNs.
- DNS panel: lookup p50/p95/p99, `fqdn_set_size`, evictions/sec.
- Conntrack/NAT panel: `thurward_conntrack_entries`, NAT port-pool
  utilization, translations/sec.
- Vsock panel: send queue depth, dropped lines/sec (alert if non-zero).

Per-flow trace exploration is a Tempo/Jaeger view linked by `flow_id`
attribute from drop events.

See [07 — Operations](07-operations.md) for example alert rules.
