# ADR 0007 — ECS-aligned log schema

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

Every accept/drop decision in thurward emits a structured log line. The
schema for those lines determines what downstream tools (Loki/Grafana,
Elastic, anything that parses logs) can do with them out of the box.

There are several established schemas: Elastic Common Schema (ECS),
Suricata's EVE JSON, CEF, OCSF. ECS is the most widely supported in
open-source dashboards and detection rule libraries; many Grafana
dashboards and SIEM rule sets target it directly.

Rolling our own schema means downstream tools need custom parsers and
custom dashboards. Adopting an existing schema means they don't.

## Decision

Emit drop/accept events as single-line JSON conforming to **Elastic
Common Schema (ECS)**, using exactly these fields:

| ECS field              | Source                                | Example                                |
| ---------------------- | ------------------------------------- | -------------------------------------- |
| `@timestamp`           | event time, RFC 3339 with millis      | `2026-05-23T18:42:11.123Z`             |
| `event.kind`           | always `event`                        | `event`                                |
| `event.category`       | always `network`                      | `network`                              |
| `event.action`         | `accept` or `drop`                    | `drop`                                 |
| `event.outcome`        | `success` (accept) or `failure` (drop)| `failure`                              |
| `rule.id`              | the matching rule's id                | `allow-github-https`                   |
| `rule.name`            | human-readable rule name              | `Allow GitHub HTTPS egress`            |
| `source.ip`            | source IP from packet                 | `10.0.0.42`                            |
| `source.port`          | source port (TCP/UDP)                 | `54231`                                |
| `destination.ip`       | destination IP from packet            | `140.82.112.4`                         |
| `destination.port`     | destination port                      | `443`                                  |
| `destination.domain`   | resolved FQDN if known (else absent)  | `github.com`                           |
| `network.transport`    | `tcp` / `udp` / `icmp`                | `tcp`                                  |
| `network.direction`    | `ingress` / `egress`                  | `egress`                               |
| `firewall.drop_reason` | thurward-specific (only on drops)     | `no_matching_allow_rule`               |

Output is single-line JSON, one event per line, sent over vsock
(see [ADR 0006](0006-vsock-for-observability-egress.md)).

## Consequences

- (+) Loki + Grafana ECS dashboards work without modification.
- (+) SIEM rule libraries (Sigma, Elastic detection rules) targeting
  ECS network events apply directly.
- (+) Stable field names mean alerting queries don't break when we
  evolve the schema — fields are added, never renamed.
- (+) `destination.domain` is populated *when* `fqdn_set` has a mapping
  for the destination IP — operators see *why* an allow happened, not
  just *that* it happened.
- (−) Some ECS fields require slightly more work to populate than a
  bespoke schema would (e.g. RFC 3339 timestamp formatting; `chrono`
  in `no_std` mode covers this in Rust).
- (−) Per-event size is larger than a packed binary format. Acceptable
  because the vsock channel has gigabits of headroom (see [ADR 0006](0006-vsock-for-observability-egress.md)).
- (○) `firewall.*` is a namespace we own; we can extend it freely
  without conflicting with ECS upstream.

## Alternatives considered

- **Bespoke compact schema.** Smaller events, faster encoding. Rejected
  because every consumer needs a custom parser and dashboards have to
  be re-authored — defeats "first-class observability".
- **Suricata EVE JSON.** Reasonable choice, used by Suricata/Zeek
  ecosystems. Rejected because ECS has broader Grafana/Loki dashboard
  support and the Elastic adoption curve is steeper.
- **OCSF (Open Cybersecurity Schema Framework).** Newer cross-vendor
  schema. Promising but smaller ecosystem of pre-built dashboards
  today. Worth revisiting in a future version.
