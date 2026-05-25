# Main acceptance scenarios

*Audience: implementers writing the data plane, reviewers checking that
behaviour matches the architecture, and operators auditing what the
firewall is supposed to do.*

This file is **black-box behavioural acceptance** for thurward. It does
not describe how to write unit tests, what framework to use, or how to
wire up traffic generators — just **what observable behaviour the
firewall must exhibit** given a rule set and a packet.

Every scenario follows the same skeleton:

```
### S-NN — short title
Reference: <chapter / ADR / rules.yaml line>
Given:     <rule set + initial state>
When:      <packet or event>
Then:      <expected verdict + counters / logs / NAT state>
```

Unless stated otherwise, scenarios assume the worked rule set in
[`examples/rules.yaml`](../examples/rules.yaml) and default action `deny`
(per [03 — Rule model](../docs/architecture/03-rule-model.md) §
"Default deny").

---

## 1. Rule matching basics

*Covers the linear first-match scan, default action, and the field
selectors defined in [03 — Rule model](../docs/architecture/03-rule-model.md).*

### S-01 — First-match-wins shadows a later deny
Reference: 03-rule-model.md § "Order matters."
Given:     `[ {action: accept, dst_port: 443, proto: tcp}, {action: deny, dst_port: 443, proto: tcp} ]`
When:      LAN client sends TCP SYN to any destination on port 443
Then:      Packet is **accepted**. The accept counter on rule 1 increments by 1; rule 2's counter is unchanged.

### S-02 — Default-deny when no rule matches
Reference: 03-rule-model.md § "Default deny"; examples/rules.yaml:6
Given:     The worked rule set
When:      LAN client sends TCP SYN to a destination on port 22 (no rule covers SSH)
Then:      Packet is **dropped**. A drop event is emitted with `rule_id` absent (or sentinel for default).

### S-03 — Source-CIDR in range vs out of range
Reference: 03-rule-model.md; examples/rules.yaml:34-41 (`allow-dns-out`)
Given:     The worked rule set
When (a):  Packet `src=10.0.0.42 → dst=10.0.0.1:53/udp`
Then (a):  Accepted by `allow-dns-out`.
When (b):  Packet `src=10.0.1.42 → dst=10.0.0.1:53/udp` (outside `10.0.0.0/24`)
Then (b):  Falls through; dropped by default-deny.

### S-04 — `destination_port` mismatch
Reference: 03-rule-model.md
Given:     The worked rule set
When:      Packet `src=10.0.0.10 → dst=10.0.0.1:5353/udp`
Then:      Does not match `allow-dns-out` (port 53 only); dropped by default-deny.

### S-05 — Protocol selector excludes other L4 protocols
Reference: 03-rule-model.md
Given:     The worked rule set
When:      Packet `src=10.0.0.10 → dst=10.0.0.1:53/tcp` (TCP, not UDP)
Then:      Does not match `allow-dns-out` (`protocol: udp`); dropped by default-deny.

### S-06 — Omitted field is a wildcard
Reference: 03-rule-model.md; examples/rules.yaml:54-58 (`allow-established` has no `source`/`destination`/`port`/`protocol`)
Given:     The worked rule set
When:      Ingress packet on a flow whose conntrack entry is `ESTABLISHED`, on any port and any protocol
Then:      Matches `allow-established` regardless of src/dst/port/proto — only `state: established` is required.

---

## 2. Direction semantics

*Covers `egress` vs `ingress` and the post-DNAT visibility of internal IPs
in [02 — Packet path](../docs/architecture/02-packet-path.md) and
[ADR 0014](../docs/architecture/decisions/0014-stateful-nat.md).*

### S-07 — `direction: egress` does not fire on a WAN-ingress packet
Reference: 02-packet-path.md § "What the parser sees"; 03-rule-model.md
Given:     A rule `{action: accept, direction: egress, dst_port: 443, proto: tcp}` and default-deny
When:      Packet arrives on the WAN interface to an internal host on TCP/443 (no DNAT entry, no conntrack entry)
Then:      Egress rule is skipped on direction mismatch; falls through to default-deny.

### S-08 — Ingress filter sees the post-DNAT internal IP
Reference: 02-packet-path.md L61-64; ADR-0014 § "Filter / NAT ordering"; examples/rules.yaml:23-28
Given:     The worked DNAT entry (`203.0.113.10:443 → 10.0.0.5:443/tcp`) and an ingress rule `{accept, direction: ingress, destination: 10.0.0.5, dst_port: 443, proto: tcp}`
When:      WAN packet `src=anywhere → dst=203.0.113.10:443/tcp`
Then:      DNAT runs first; the filter sees `dst=10.0.0.5:443` and the ingress rule matches.

### S-09 — Ingress rule keyed on the WAN IP does NOT match
Reference: ADR-0014 § "Filter / NAT ordering"
Given:     The worked DNAT entry plus an ingress rule keyed on `destination: 203.0.113.10`
When:      WAN packet `src=anywhere → dst=203.0.113.10:443/tcp`
Then:      The rule does **not** match (filter sees post-DNAT `10.0.0.5`). Falls through to default-deny.

---

## 3. Stateful conntrack

*Covers `state: established` and the conntrack lifecycle from
[ADR 0014](../docs/architecture/decisions/0014-stateful-nat.md) and
the timeouts in `examples/rules.yaml:11-16`.*

### S-10 — Return traffic on an established flow is accepted
Reference: examples/rules.yaml:54-58 (`allow-established`)
Given:     An egress accept rule on TCP/443 plus `allow-established`
When:      LAN client completes a TCP handshake to a WAN host on port 443, then the WAN host sends a data segment back
Then:      The egress SYN is accepted (rule + conntrack entry created). The WAN→LAN reply matches `allow-established`.

### S-11 — Unsolicited WAN→LAN packet hits default-deny
Reference: 03-rule-model.md § "state: established"
Given:     Only `allow-established` (no other ingress rule); conntrack table is empty
When:      WAN sends an unsolicited TCP SYN to a LAN host on a port allowed by some *egress* rule
Then:      No conntrack entry exists → `allow-established` does not fire → default-deny drops the packet.

### S-12 — TCP conntrack state machine and timeouts
Reference: examples/rules.yaml:11-16 (`tcp_syn_sent`, `tcp_established`, `tcp_time_wait`)
Given:     The default conntrack timeouts
When:      A TCP flow goes SYN → SYN-ACK → ACK (handshake), idles, then sees FIN/FIN-ACK
Then:      State transitions SYN_SENT → ESTABLISHED → TIME_WAIT. Entry ages out at `tcp_time_wait` after FIN; a new SYN on the same 5-tuple installs a fresh entry.

### S-13 — UDP entry ages out at `udp_idle`
Reference: examples/rules.yaml:15
Given:     A UDP flow accepted by an egress rule; `udp_idle = 60`
When:      Flow is idle for >60 s, then the LAN client sends another UDP packet on the same 5-tuple
Then:      The original entry is gone; a fresh conntrack entry is created and the egress rule re-evaluates (and re-accepts).

### S-14 — ICMP echo reply matches echo request entry
Reference: examples/rules.yaml:16 (`icmp_echo`)
Given:     An egress accept rule for ICMP and `allow-established` for ingress
When:      LAN client emits ICMP echo request to a WAN host within `icmp_echo` seconds, host replies
Then:      Egress request creates the entry. The ingress echo reply matches the entry → `allow-established` fires → accepted.

### S-15 — Conntrack table full: new flow dropped, pressure metric increments
Reference: 02-packet-path.md § "Performance notes" (conntrack is open-addressed hash)
Given:     Conntrack table at capacity
When:      A new flow's first packet arrives
Then:      Packet is dropped (no silent bypass). A `conntrack_pressure_drops` metric increments.

---

## 4. SNAT / masquerade

*Covers the SNAT block in `examples/rules.yaml:18-22` and the
deterministic per-flow port allocation defined in
[ADR 0014](../docs/architecture/decisions/0014-stateful-nat.md).*

### S-16 — Egress packet is SNAT'd to the WAN IP
Reference: examples/rules.yaml:18-22
Given:     The worked SNAT block (`source: 10.0.0.0/24`, `masquerade: true`, `out_interface: wan0`)
When:      LAN packet `src=10.0.0.42:50000 → dst=1.2.3.4:443/tcp` is accepted by the filter
Then:      Packet leaves `wan0` with `src=<wan0_primary_ip>:<port-from-pool>`. A conntrack entry binds the pre-translation 5-tuple to the chosen WAN port.

### S-17 — Same flow re-uses its allocated source port
Reference: ADR-0014 § port-allocation determinism
Given:     A flow already has an SNAT binding
When:      A second packet on the same pre-translation 5-tuple is forwarded
Then:      The same external source port is used (binding is deterministic per pre-translation 5-tuple).

### S-18 — Reverse-translation on the return packet
Reference: ADR-0014
Given:     An established SNAT'd flow
When:      The WAN host replies to the WAN IP at the allocated port
Then:      Conntrack reverse-translation rewrites `dst` back to the original internal IP and port before the packet leaves the LAN interface.

### S-19 — SNAT port pool exhaustion drops new flows
Reference: ADR-0014 § port pool; 09-limitations.md § "CGNAT-scale port allocation"
Given:     The SNAT port pool for `wan0_primary_ip` is fully allocated
When:      A new internal flow attempts to traverse
Then:      The new flow is dropped (not silently overlapped onto an existing binding). A `snat_pool_exhaustion` metric increments.

---

## 5. DNAT / port-forward

*Covers the DNAT block in `examples/rules.yaml:23-28` and the
limitations called out in
[09 — Limitations](../docs/architecture/09-limitations.md) § "NAT scope".*

### S-20 — WAN packet to the DNAT target is rewritten before filtering
Reference: ADR-0014 § "Filter / NAT ordering"; covered for the matching half by S-08
Given:     The worked DNAT entry
When:      WAN packet `dst=203.0.113.10:443/tcp` arrives
Then:      DNAT rewrites `dst` to `10.0.0.5:443` *before* the filter scan. The filter sees the post-DNAT tuple.

### S-21 — DNAT `from:` CIDR filter
Reference: examples/rules.yaml:28 (`from: 0.0.0.0/0`); ADR-0014
Given:     The DNAT entry with `from: 198.51.100.0/24`
When (a):  WAN packet `src=198.51.100.7 → dst=203.0.113.10:443/tcp`
Then (a):  DNAT applies; filter accepts (assuming a matching ingress rule).
When (b):  WAN packet `src=203.0.113.99 → dst=203.0.113.10:443/tcp` (outside `from:`)
Then (b):  DNAT does **not** apply; filter sees the original WAN IP and falls through to default-deny.

### S-22 — Hairpin is unsupported
Reference: 09-limitations.md § "Hairpin NAT"
Given:     The worked DNAT entry and a LAN client at `10.0.0.42`
When:      LAN client connects to `203.0.113.10:443` (its own WAN IP)
Then:      The connection does **not** loop back through DNAT. The packet is dropped or otherwise fails — this is the documented v1 behaviour, not a bug.

### S-23 — Symmetric reverse-translation on DNAT reply
Reference: ADR-0014
Given:     An established DNAT'd flow (per S-20)
When:      The internal target `10.0.0.5:443` replies
Then:      Reverse-translation rewrites the source back to `203.0.113.10:443` before the packet leaves `wan0`.

---

## 6. FQDN rules and DNS proxy

*Covers FQDN matching and DNS-proxy behaviour from
[04 — FQDN & DNS](../docs/architecture/04-fqdn-and-dns.md) and the
`allow-github-https` rule in `examples/rules.yaml:43-52`.*

### S-24 — FQDN rule matches only after the client resolves through the proxy
Reference: 04-fqdn-and-dns.md § "DNS-proxy flow"
Given:     The worked rule set including `allow-github-https`
When:      LAN client first asks `10.0.0.1:53` for `github.com`, then connects to the returned IP on TCP/443
Then:      The DNS proxy populates `fqdn_set` with `(returned_ip → github.com, expiry=now+ttl)`. The TCP SYN's `dst_ip` hits `fqdn_set` → rule matches → accepted.

### S-25 — Hard-coded IP bypasses the FQDN rule
Reference: 04-fqdn-and-dns.md § "Pitfalls"
Given:     The worked rule set; client has not queried DNS through the proxy
When:      LAN client connects directly to GitHub's IP `140.82.x.x:443/tcp`
Then:      `fqdn_set` lookup misses → `allow-github-https` does not fire → falls through to default-deny.

### S-26 — Wildcard matches one label deep, not the apex, not multi-label
Reference: 03-rule-model.md § "destination_fqdn"; 04-fqdn-and-dns.md
Given:     Rule lists `*.githubusercontent.com`
When (a):  `raw.githubusercontent.com` resolves and the client connects
Then (a):  Wildcard matches (one label deep).
When (b):  `githubusercontent.com` resolves and the client connects
Then (b):  Apex does **not** match the wildcard alone (the rule also lists `github.com` separately for the apex; without that, the apex misses).
When (c):  `a.b.githubusercontent.com` resolves and the client connects
Then (c):  Two-label-deep name does **not** match `*.githubusercontent.com`.

### S-27 — CNAME chain uses the final-leaf TTL
Reference: 04-fqdn-and-dns.md § "CNAME chains"
Given:     `assets.example.com` (TTL 300) is a CNAME to `cdn.provider.net` (TTL 30) which resolves to an A record (TTL 30)
When:      Client resolves `assets.example.com` through the proxy
Then:      Every link in the chain is recorded in `fqdn_set` with expiry = `now + 30 s` (final-leaf TTL), not 300.

### S-28 — TTL expiry causes cache miss until next resolve
Reference: 04-fqdn-and-dns.md § "Expiry: lazy"
Given:     An `fqdn_set` entry has expired
When:      A LAN client (any client, the cache is shared) connects to the formerly-bound IP
Then:      Lookup returns a miss → FQDN rule does not match → falls through (eventually to default-deny). The next DNS query through the proxy re-installs the binding.

### S-29 — DoH / DoT / hardcoded resolver bypasses FQDN policy
Reference: 04-fqdn-and-dns.md § "Pitfalls"; 09-limitations.md § "DNS bypass"
Given:     The worked rule set; client uses DoH to a remote resolver and connects to the resolved IP
Then:      thurward never observes the DNS exchange → no `fqdn_set` entry → FQDN rules never match. This is the documented v1 limitation, not a defect. Mitigation lives operationally outside thurward.

### S-30 — Truncated UDP response triggers TCP fallback
Reference: 04-fqdn-and-dns.md § "UDP truncation handling"
Given:     Upstream returns a UDP response with `TC=1`
When:      Client asked over UDP/53
Then:      The proxy re-issues the query over TCP/53 upstream, receives the full response, and returns it to the client (over UDP if it fits, else TCP). `fqdn_set` is populated from the TCP response.

### S-31 — NXDOMAIN / SERVFAIL passthrough, no negative caching
Reference: 04-fqdn-and-dns.md § "NXDOMAIN / SERVFAIL passthrough"
Given:     Upstream returns NXDOMAIN for a name
When:      Client queries the proxy
Then:      The proxy passes NXDOMAIN through unchanged. No `fqdn_set` entry. A second query for the same name re-asks upstream (no negative cache).

### S-32 — Per-client DNS QPS limit
Reference: 04-fqdn-and-dns.md § "Per-client rate-limiting"; examples/rules.yaml:10 (`dns_qps_per_client: 100`)
Given:     A LAN client emits 200 distinct DNS queries within one second
When:      The proxy processes them
Then:      Up to 100 are forwarded upstream; the rest receive SERVFAIL. `fqdn_set` is not poisoned (no entries from rejected queries).

### S-33 — EDNS Client Subnet stripped on forward
Reference: 04-fqdn-and-dns.md § "Out of scope (v1)"
Given:     A client query containing an EDNS Client Subnet option
When:      The proxy forwards it upstream
Then:      The ECS option is stripped (not relayed). The client's IP is not leaked to upstream.

### S-34 — `fqdn_set` capacity eviction is by oldest, regardless of TTL
Reference: 04-fqdn-and-dns.md § "fqdn_set cache" (capacity bounded at default 32k)
Given:     `fqdn_set` is full
When:      A new resolution must be inserted
Then:      The oldest entry is evicted even if its TTL has not expired.

---

## 7. Address family and packet validity

*Covers the parser-stage drops in
[02 — Packet path](../docs/architecture/02-packet-path.md) and the
unsupported-family policy in
[09 — Limitations](../docs/architecture/09-limitations.md).*

### S-35 — IPv6 is dropped unconditionally
Reference: 02-packet-path.md L104-105; 09-limitations.md § "IPv4 only"
Given:     Any rule set
When:      An IPv6 frame arrives on either interface
Then:      Dropped at the Ethernet/IP type check. A drop event with a v6-specific reason is emitted. No filter scan runs.

### S-36 — IPv4 fragments are dropped (no reassembly in v1)
Reference: 02-packet-path.md L106-108
Given:     Any rule set
When:      An IPv4 fragment (More-Fragments or non-zero offset) arrives
Then:      Dropped at the parse stage. A drop event with a fragment-specific reason is emitted.

### S-37 — Malformed IPv4 header is dropped before the rule scan
Reference: 02-packet-path.md L80-84 (`EthernetFrame::new_checked`); L106-108
Given:     Any rule set
When:      A frame with a truncated IP header, impossible total length, or bad header checksum arrives
Then:      `smoltcp::wire` parser fails → `DropReason::Malformed` → drop event emitted. No conntrack lookup, no rule scan.

### S-38 — Non-IPv4 EtherType
Reference: 02-packet-path.md L104-105 (Ethernet header confirms IPv4)
Given:     Any rule set
When:      A non-IPv4 EtherType frame arrives (e.g. raw ARP from the LAN)
Then:      The Ethernet check rejects it as non-forwardable. Drop event emitted with a corresponding reason. *(Implementer note: confirm whether ARP needs special handling for the firewall's own MAC/IP; if so, document it here as the expected behaviour.)*

---

## 8. Observability side-effects

*Covers the post-decision emit path from
[05 — Observability](../docs/architecture/05-observability.md),
[ADR 0006](../docs/architecture/decisions/0006-vsock-for-observability-egress.md),
[ADR 0007](../docs/architecture/decisions/0007-ecs-aligned-log-schema.md), and
[ADR 0008](../docs/architecture/decisions/0008-per-flow-tracing-not-per-packet.md).*

### S-39 — Every decision emits exactly one ECS JSON event
Reference: 02-packet-path.md § "Performance notes" (post-decision emit); ADR-0007
Given:     Any rule set
When:      The filter produces a verdict (accept or drop) for a packet
Then:      Exactly one ECS-aligned JSON event is queued for vsock egress, after the verdict. The hot path does not block on emission.

### S-40 — Per-rule counter increments exactly once per match
Reference: 03-rule-model.md § "Per-rule counters"
Given:     A rule with a known counter index
When:      N packets match that rule
Then:      The rule's `(accept_count | drop_count)` counter ends at exactly N higher than its starting value. No double-counting on retransmits — counters are bumped per matched packet, not per flow.

### S-41 — Flow trace sampling honours configured rates
Reference: ADR-0008; examples/rules.yaml:8-9 (`flow_trace_sample_accept: 0.01`, `flow_trace_sample_drop: 1.0`)
Given:     The worked defaults
When:      10 000 distinct accepted flows are observed and 100 distinct dropped flows are observed
Then:      Roughly 100 accepted flow spans are emitted (±sampling jitter); all 100 dropped flow spans are emitted.

### S-42 — Slow vsock consumer does not gate the data path
Reference: 02-packet-path.md § "Performance notes"; 05-observability.md (vsock backpressure drops oldest)
Given:     The vsock consumer is paused
When:      Traffic continues to traverse the firewall
Then:      Forwarding latency on the hot path is unaffected. Oldest queued observability lines are dropped to keep the queue bounded.

---

## 9. Build-time guarantees

*Covers the compiled-rules model in
[ADR 0005](../docs/architecture/decisions/0005-build-time-rule-compilation.md) and
the schema-first contract in
[ADR 0011](../docs/architecture/decisions/0011-schema-first-iac-no-custom-provider.md).
These scenarios assert what fails the `cargo build`, not the running image.*

### S-43 — Missing `defaults.default_action` fails the build
Reference: 03-rule-model.md § "Default deny"; schemas/rules.schema.json
Given:     `rules.yaml` with no `defaults.default_action`
When:      `build.rs` validates against the schema
Then:      Build fails with a precise error pointing at the missing field.

### S-44 — Unknown protocol value fails schema validation
Reference: schemas/rules.schema.json (protocol enum)
Given:     A rule with `protocol: sctp` (or any value outside the enum)
When:      `build.rs` validates
Then:      Build fails; the running image does not contain the offending rule.

### S-45 — Duplicate rule `id` fails the build
Reference: 03-rule-model.md (ids are stable references in counters/logs)
Given:     Two rules sharing the same `id`
When:      `build.rs` compiles `rules_table.rs`
Then:      Build fails. Justification: stable rule ids back per-rule counters and ECS event fields; collisions would corrupt both.

### S-46 — Invalid wildcard FQDN syntax fails the build
Reference: 03-rule-model.md § "destination_fqdn" (exact + `*.domain` only)
Given:     A rule with `destination_fqdn: ["**.foo.com"]` or `"foo.*.com"` (anything outside the documented grammar)
When:      `build.rs` validates
Then:      Build fails before `rules_table.rs` is written.

### S-47 — No runtime rule-reload surface exists
Reference: 09-limitations.md § "No hot rule reload"; ADR-0016
Given:     A running thurward image
When:      An operator sends `SIGHUP`, writes to any conceivable control socket, or hits any port other than the documented data ports
Then:      Nothing changes. There is no admin API. The only way to change rules is to build a new image and restart the launcher. This is asserted by **absence** — the test confirms no listening admin endpoints, sockets, or signal handlers respond to reload attempts.

---

## 10. Security and integrity controls (standards-driven)

*Covers behaviours surfaced by mapping `scenarios.md` to NIST SP 800-41
Rev 1, NIST SP 800-53 Rev 5, ISO/IEC 27001:2022 Annex A, CIS Controls
v8.1, and SLSA v1.0 (see Appendix A). These scenarios assert behaviours
that the architecture docs imply but had not previously been tested.*

### S-48 — Default-deny holds on cold start before rules are evaluable
Reference: NIST SP 800-53 SC-7(5); 09-limitations.md § "No HA" ("default deny holds"); ADR-0005 (build-time rules embedded as `const`)
Given:     A thurward image is booted from cold
When:      The first packet arrives before the rule table is fully accessible (race window during init)
Then:      The packet is dropped. There is no window between launcher start and first rule evaluation in which packets are forwarded uninspected. Asserted by injecting traffic during the first 100 ms of the boot probe (benchmarks.md B-23) and confirming all packets in that window appear as drop events once observability comes online.

### S-49 — Log timestamps are NTP-disciplined and timezone-explicit
Reference: NIST SP 800-53 AU-8; ISO/IEC 27001:2022 Annex A 8.17; ADR-0007 (ECS-aligned log schema)
Given:     A running thurward image with a working time source
When:      Any accept or drop event is emitted on vsock
Then:      Each ECS event contains a `@timestamp` field in UTC ISO-8601 with at least millisecond precision (`2026-05-25T14:37:21.123Z`), and the image's time source is identifiable to the operator (NTP server, PTP master, or an explicit `clock_source: none` marker if synchronisation is unavailable). Untrusted timestamps invalidate log evidence in audit contexts.

### S-50 — Launcher refuses to boot an image whose signature does not verify
Reference: NIST SP 800-53 SI-7, CM-5; SLSA v1.0 § Build L3 (provenance verification); ADR-0010 (supply chain hardening); ADR-0016 (image-per-change deploy)
Given:     A signed thurward image and the operator's published cosign public key
When (a):  The operator launches an unmodified, validly-signed image
Then (a):  Boot proceeds normally.
When (b):  The operator launches an image whose signature does not verify (modified bits, expired cert, wrong key)
Then (b):  The launcher **refuses to start** and emits a verification-failure event to the host journal. The signed-build promise is enforced at boot, not just at publish.

### S-51 — Build provenance links the running image to source `rules.yaml`
Reference: NIST SP 800-53 AU-2; SLSA v1.0 provenance; ADR-0005, ADR-0010
Given:     A running thurward image and its associated SLSA provenance attestation
When:      An auditor wants to verify which rule set is running
Then:      The provenance attestation names the git commit of `rules.yaml` (and `schemas/rules.schema.json`) used at build time, signed by the build system. The auditor can fetch the source rules at that commit and reproduce the build to confirm. No image is published without an attestation.

---

## Appendix A: Mapping to security and IT standards

*Maps the behavioural scenarios above to the control IDs auditors and
security reviewers actually ask about. Grouped by standard; each row
names the control and lists the S-NN that satisfy it. A scenario can
appear under multiple controls.*

### NIST SP 800-41 Rev 1 — Guidelines on Firewalls and Firewall Policy

| Section / topic | Scenarios |
| --- | --- |
| §2.2 Deny-by-default posture | S-02, S-48 |
| §2.4 Stateful inspection | S-10, S-11, S-12, S-13, S-14 |
| §2.5 Rule ordering (first-match-wins) | S-01 |
| §2.6 NAT semantics (source / destination translation) | S-08, S-09, S-16, S-17, S-18, S-19, S-20, S-21, S-22, S-23 |
| §3 Logging and monitoring | S-39, S-40, S-41, S-49 |
| §4 Change management (no runtime admin) | S-47, S-50, S-51 |

### NIST SP 800-53 Rev 5 — Security and Privacy Controls

| Control | Title | Scenarios |
| --- | --- | --- |
| SC-7 | Boundary Protection | S-02, S-07, S-08, S-09, S-22, S-25, S-29, S-35, S-36, S-37, S-48 |
| SC-7(5) | Deny by default | S-02, S-48 |
| AU-2 | Event Logging | S-39, S-40, S-51 |
| AU-3 | Content of Audit Records | S-39 |
| AU-8 | Time Stamps | S-49 |
| AU-12 | Audit Record Generation | S-39, S-40, S-41 |
| SI-4 | System Monitoring | S-39, S-40, S-41, S-42 |
| SI-7 | Software / Firmware Integrity | S-50 |
| CM-5 | Access Restrictions for Change | S-47, S-50, S-51 |
| AC-3(7) | Role-based / least-functionality access enforcement | S-47 |

### ISO/IEC 27001:2022 Annex A — Information Security Controls

| Control | Title | Scenarios |
| --- | --- | --- |
| 8.15 | Logging | S-39, S-40, S-41, S-49 |
| 8.17 | Clock synchronization | S-49 |
| 8.20 | Networks security | S-02, S-07, S-08, S-09, S-35, S-36, S-37, S-48 |
| 8.21 | Security of network services | S-24, S-25, S-29, S-30, S-31, S-32, S-33 |
| 8.22 | Segregation of networks | S-02, S-07, S-22 |
| 8.32 | Change management | S-47, S-50, S-51 |

### CIS Controls v8.1

| Control | Title | Scenarios |
| --- | --- | --- |
| 3.3 | Configure Data Access Control Lists | S-01, S-02, S-03, S-04, S-05, S-06, S-07, S-08, S-09 |
| 4.x | Secure Configuration of Enterprise Assets and Software | S-43, S-44, S-45, S-46, S-50 |
| 8.x | Audit Log Management | S-39, S-40, S-41, S-42, S-49 |
| 12.x | Network Infrastructure Management | S-02, S-08, S-09, S-22, S-47 |
| 13.x | Network Monitoring and Defense | S-39, S-40, S-42 |

### PCI DSS v4.0.1 — applicable if thurward is deployed at a CDE boundary

| Requirement | Topic | Scenarios |
| --- | --- | --- |
| 1.2 | Configuration of network security controls (NSCs) | S-01, S-02, S-43, S-44, S-45, S-46 |
| 1.3 | Restrict inbound / outbound traffic between trusted and untrusted | S-02, S-07, S-08, S-09, S-48 |
| 1.4 | Network connections between trusted and untrusted | S-02, S-22, S-29 |
| 10.x | Logging and monitoring | S-39, S-40, S-49 |
| 11.5 | Change detection on critical files | S-50, S-51 |

### SLSA v1.0 — Supply-chain Levels for Software Artifacts

| Track / Level | Topic | Scenarios |
| --- | --- | --- |
| Build L3 | Provenance generated, signed, non-falsifiable | S-50, S-51 |
| Source L2 | Version-controlled rules with two-party review of `rules.yaml` | S-51 (provenance trace) |

### Coverage notes

- Every scenario S-01..S-51 appears in at least one row above **except** scenarios that are pure implementation details rather than auditable controls: S-17 (SNAT port reuse determinism), S-26 b/c (wildcard depth semantics), S-28 (TTL expiry), S-34 (cache eviction policy), S-38 (non-IPv4 EtherType handling), S-12 b–c (TCP state transitions). These are behavioural assertions that contribute to higher-level controls indirectly but don't map to a control ID of their own.
- Controls covered by **many** scenarios indicate defence-in-depth (e.g. SC-7 → 11 scenarios, ISO 8.20 → 8 scenarios).
- Controls covered by **one** scenario are candidates for additional coverage in future passes (e.g. SI-7 → S-50 only; AU-8 → S-49 only).
- Controls explicitly **not covered** in this file (out of scope or out of thurward's architectural envelope): AU-9 (audit info protection — operational, not a firewall behaviour), CP-* (contingency planning — out of scope per 09-limitations.md "No HA"), IR-* (incident response — operational), AC-2/AC-7 (user account management — thurward has no users).

---

## Cross-cutting verification notes

- Every "Reference:" line above points at a real source-of-truth
  location — a chapter, ADR, `rules.yaml` line, RFC section, or
  named control ID. `grep`-ing the file should resolve each citation.
- Every limitation documented in
  [09 — Limitations](../docs/architecture/09-limitations.md) that has
  observable behaviour gets at least one asserted scenario:
  IPv6 (S-35), hairpin NAT (S-22), DNS bypass (S-29), no hot reload
  (S-47), no L7 inspection (asserted implicitly by the absence of any
  L7 match field in S-04..S-06). HA, fleet rollout, throughput, ARM,
  bare-metal, and toolchain pinning are non-behavioural and therefore
  not in this file.
- Every scenario covered by Appendix A maps to at least one named
  control in NIST SP 800-41, SP 800-53, ISO 27001 Annex A, CIS v8.1,
  PCI DSS v4.0.1, or SLSA v1.0. Scenarios not mapped are flagged
  explicitly in the Coverage notes above.
- Any rule snippet quoted in a scenario must remain valid against
  [`schemas/rules.schema.json`](../schemas/rules.schema.json). The CI
  step that validates `examples/rules.yaml` against the schema should
  be extended to validate inline snippets in this file once the test
  infrastructure exists.
- Once a data plane exists, each S-NN above becomes an integration
  test, and the Appendix A table becomes the audit artifact reviewers
  reach for. The file's job today is to be the spec both audiences
  are written against.
