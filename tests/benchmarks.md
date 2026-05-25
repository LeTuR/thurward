# Performance benchmarks and open-source firewall comparison

*Audience: implementers tuning the data plane, reviewers checking the
v1 envelope holds, and operators deciding whether thurward fits their
throughput / latency budget vs. existing options.*

This file is **black-box performance acceptance** for thurward and a
**comparison spec** against four open-source firewalls. It does not
prescribe a harness, traffic-generator config, or CI wiring — just
**what workload is run, what is measured, what counts as passing, and
how the comparison candidates are expected to land**.

Companion to [`scenarios.md`](./scenarios.md) (behavioural acceptance).

Every benchmark follows the same skeleton:

```
### B-NN — short title
Reference: <chapter / ADR / RFC / standard>
Workload:  <traffic profile + generator>
Metric:    <what is measured and how>
Pass:      <thurward's pass criterion vs. its own v1 envelope>
Compare:   <per-candidate expectation>
```

---

## 0. Methodology

**Implementation harness:** [`bench/`](../bench/) implements the
methodology below. Tier-0 (workstation + virtio) ships today; Tier-1
(physical NIC + SR-IOV + RT kernel) is the path described in
[`bench/ROADMAP.md`](../bench/ROADMAP.md). Every result JSON written
by the harness conforms to [`bench/results/SCHEMA.md`](../bench/results/SCHEMA.md).

### 0.1 Standards alignment

This spec follows, in order of authority:

- **RFC 9411** (2023) — *Benchmarking Methodology for Network Security Device Performance*, NetSecOPEN-co-authored. The five-phase test lifecycle (§0.5 below), error budgets (§0.10), and security-effectiveness precondition (§0.8) come from here. <https://datatracker.ietf.org/doc/rfc9411/>
- **RFC 2544** (1999) — *Benchmarking Methodology for Network Interconnect Devices*. Frame-size sweep (§9), trial duration (§24), throughput / latency / loss / burst / recovery test definitions (§26.1–26.5). <https://datatracker.ietf.org/doc/html/rfc2544>
- **draft-ietf-bmwg-mlrsearch** — Multiple Loss Ratio Search; modern replacement for RFC 2544's binary search. Used by TRex and FD.io CSIT. <https://datatracker.ietf.org/doc/draft-ietf-bmwg-mlrsearch/>
- **draft-ietf-bmwg-containerized-infra** — virtualisation-environment invariants (NUMA, hugepages, NIC offloads, queue counts). <https://www.ietf.org/archive/id/draft-ietf-bmwg-containerized-infra-02.html>
- **RFC 6815** (2012) — applicability statement. All BMWG work is **lab-only**; numbers produced under this spec are not safe to gather on production networks. <https://datatracker.ietf.org/doc/html/rfc6815>
- **FD.io CSIT methodology** for the `ip4base` control-run pattern (§0.6 / B-00) and NAT44 session-scale conventions. <https://docs.fd.io/csit/master/report/vpp_performance_tests/methodology.html>

Deviations from RFC 9411 are catalogued explicitly in §0.11.

### 0.2 Comparison set

| Candidate                | Substrate                                                       | Why it's here                                                                                                  |
| ------------------------ | --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **thurward**             | Hermit unikernel + Rust + smoltcp::wire on QEMU/KVM virtio-net  | System under test.                                                                                              |
| **Linux nftables**       | Linux kernel netfilter + conntrack on KVM virtio-net            | Closest apples-to-apples peer: same VM substrate, same NIC model, conntrack + NAT in scope.                     |
| **OPNsense / pf**        | FreeBSD + pf on KVM virtio-net                                  | Popular open-source edge firewall; different OS, same role and audience.                                        |
| **VyOS**                 | Linux router distribution on KVM virtio-net                     | Turnkey-VM-appliance comparison; same kernel class as nftables but with a router-shaped UX.                     |
| **VPP (FD.io)**          | Userspace DPDK on kernel-bypass NIC                             | **Ceiling reference only.** Different substrate, different NIC model — reported as an upper-bound, not a fair head-to-head. |

The substrate-comparison framing follows the IncludeOS Sci. Reports 2024 study (Bratterud et al., <https://www.nature.com/articles/s41598-024-51167-8>) — the closest published prior art for a unikernel-vs-kernel-firewall benchmark.

### 0.3 Test environment (apples-to-apples)

- Identical VM spec across thurward / nftables / OPNsense / VyOS: same vCPU count (1 for envelope tests, 2/4 for stretch tests in B-05), same RAM, same virtio-net queue count, same host NIC, same hypervisor (KVM/QEMU per [06 — Deployment](../docs/architecture/06-deployment.md)).
- Traffic generator runs on a **separate host** (or a sibling VM with DPDK passthrough), never on the same host as the SUT.
- Each candidate is loaded with the **equivalent translation** of [`examples/rules.yaml`](../examples/rules.yaml) per §0.7 below.

### 0.4 Host invariants (BMWG containerised-infra)

Every benchmark run records, and every candidate run holds **identical**, the following host-side invariants. A mismatch on any one of these invalidates the comparison and the run is rerun.

- **NUMA placement** — VM, vCPUs, virtio backend threads, and physical NIC pinned to the same NUMA node. Cross-NUMA placement is documented as commonly costing > 10% (containerised-infra draft); we never let it confound a comparison.
- **vCPU pinning** — `taskset` / hypervisor pin to dedicated host cores; host scheduler does not migrate them. Pinned cores `isolcpus`'d or equivalent.
- **Hugepages** — explicit on/off state recorded per run; identical across candidates within a comparison group.
- **NIC offloads** — TSO, GSO, GRO, checksum offload all explicitly enabled or disabled per run; identical across candidates. UDP 64 B is unaffected; TCP throughput is dominated by them.
- **virtio-net queue count** — single-queue for v1 envelope tests; multi-queue runs are flagged as v1.x stretch in B-05.

### 0.5 Trial structure (RFC 9411 §4.3.4)

Every benchmark run consists of five phases:

1. **Initialisation** — ≥ 5 s. Cold caches, conntrack empty, FQDN cache empty unless the benchmark explicitly pre-warms.
2. **Ramp-up** — gradual to target offered load. Allows branch predictors, conntrack hash, and FQDN cache to reach steady state.
3. **Sustain** — **≥ 300 s**, samples taken at intervals **≤ 2 s**. This is where the headline numbers are measured. Sustain-phase stability proves we're not reporting a transient peak.
4. **Ramp-down** — return to idle.
5. **Collection** — drain in-flight observability events; assemble per-trial metrics.

Wallclock-only benchmarks (B-23 launch, B-20 recovery) are not subject to this lifecycle and say so in their `Workload:` line.

### 0.6 Reference path (`ip4base` control)

Before any candidate benchmark, the same test is run **against a no-rules, no-NAT pass-through configuration** ([B-00](#b-00--reference-path-with-empty-ruleset)) at every frame size. This proves the traffic generator can saturate the wire — if B-00 is itself bottlenecked, downstream B-NN numbers reflect the generator, not the SUT. Pattern lifted from FD.io CSIT.

### 0.7 Translation discipline

Each candidate is configured with its **idiomatic structured form** of [`examples/rules.yaml`](../examples/rules.yaml) — not a forced flat list:

- **thurward** — the YAML compiled to a Rust `const` rule table (ADR-0005).
- **nftables** — verdict maps, sets, and named tables. A flat-list nftables ruleset gives wildly different numbers vs. structured nftables (Ding 2018, Red Hat 2017); comparing the wrong form is methodologically incorrect.
- **OPNsense / pf** — pf tables, anchors, and quick rules.
- **VyOS** — firewall groups and address-groups.
- **VPP** — classifier ACLs / hash tables.

The four translations live alongside the future benchmark harness; this file references them by name. Each translation is reviewed for **functional equivalence** to `examples/rules.yaml` — same default-deny, same NAT setup, same conntrack timeouts where configurable, no DPI/L7/IDS enabled.

### 0.8 Security-effectiveness precondition (RFC 9411 §4.2.1, Appendix A)

**Every image under test must first pass the corresponding [`scenarios.md`](./scenarios.md) acceptance run.** If any S-NN fails on a given image, performance numbers from that image are invalidated and not reported. This rule applies equally to thurward and to every comparison candidate: a fast firewall that doesn't actually enforce its rules is not a firewall.

### 0.9 Tools

- **TRex** (open-source, Cisco) — stateful and ASTF workloads; MLRsearch implementation; CPS, concurrent connections, latency percentiles.
- **pktgen-dpdk** / **MoonGen** — raw pps at small frame sizes for §0.6 control and B-1a curve.
- **iperf3** — TCP throughput sanity check; cross-references against IncludeOS-paper-style numbers.
- **hping3** — small-N latency sanity, IncludeOS-paper-compatible (B-24).
- **Wallclock + systemd journal** — boot and recovery timing (B-19/B-23/B-20).

### 0.10 Search algorithm and loss tolerance

The throughput-finding algorithm is **MLRsearch** (`draft-ietf-bmwg-mlrsearch`), which discovers two rates per run:

- **NDR** — Non-Drop Rate: highest offered rate with **Packet Loss Ratio (PLR) = 0** over the sustain phase. Equivalent to RFC 2544 "throughput".
- **PDR** — Partial Drop Rate: highest offered rate with PLR ≤ a named threshold. **Default PLR = 0.001%** (NetSecOPEN convention; RFC 9411 error budget). Benchmarks that pick a different PLR threshold state it explicitly.

"Zero-loss" in this spec means **0 lost frames** in the sustain window, not "≤ a few".

### 0.11 Reporting discipline

- Each benchmark cell in [§9](#9-comparison-summary) reports **median + p95 across N trials** with N ≥ 5 unless the benchmark says otherwise. N is stated once here and overridden per benchmark only when justified.
- **Latency** is reported as the average of **≥ 20 trials** (RFC 2544 §26.2) at the throughput rate, plus p50 / p99 / p99.9 from the trial-aggregated histogram.
- **CPU utilisation** on the SUT is recorded per benchmark.
- **Throughput per vCPU** (Mpps/vCPU or Gbps/vCPU) is reported as a derived efficiency column in §8 — distinguishes thurward from VPP on the metric that actually matters for a 1-vCPU edge firewall.
- **Host invariants** (§0.4) are recorded with every result. A result whose host invariants don't match its comparison cohort is rerun, not patched.

### 0.12 Deviations from RFC 9411

Conscious departures, with the architectural justification:

- **No L7 / DPI tests** — thurward filters 5-tuple + FQDN-cache binding; HTTP TPS, TLS handshake rate, and inspected-throughput tests from RFC 9411 §7.3/§7.6/§7.7 are out of scope.
- **No QUIC / HTTP/3 tests** — same reason.
- **No HTTP object-size mix (RFC 9411 Tables 5)** — not applicable to an L3/L4 firewall.
- **No NetSecOPEN vertical traffic mixes (Education, Healthcare)** — those are HTTPS-heavy and exercise the L7 path we don't have.
- **IP fragmentation explicitly tested as a drop case** (B-NN n/a; tested in `scenarios.md` S-36 instead). RFC 9411 removed fragmentation perf tests; we agree.

### 0.13 What's excluded (per `09-limitations.md`)

The following are **not benchmarked** because they're out of scope for v1:

- IPv6 (dropped unconditionally; see scenarios.md S-35)
- IPv4 fragments (dropped, no reassembly; see scenarios.md S-36)
- Hairpin NAT, ALGs, CGNAT, NAT64/66
- ARM, bare-metal direct boot
- HA / failover
- L7 / DPI / TLS inspection

Including these would compare nothing (thurward drops them) or apples to oranges (peers do them, thurward intentionally doesn't).

### 0.14 Lab-only (RFC 6815)

This spec is **lab-only**. Running these tests against production networks is considered harmful; numbers gathered in production are not valid under this methodology.

---

## 1. Forwarding throughput

*Validates the [02 — Packet path](../docs/architecture/02-packet-path.md)
§ "Performance notes" envelope of ~1 Gbps on a single vCPU.*

### B-00 — Reference path with empty ruleset
Reference: FD.io CSIT `ip4base` methodology
Workload:  Same traffic profile as B-01..B-04 but against a candidate with **no rules and no NAT** (pass-through). Run at all seven RFC 2544 frame sizes.
Metric:    NDR / PDR (Mpps, Gbps) per frame size
Pass:      Generator saturates the wire (or comes within a documented margin). If B-00 itself is loss-limited, **no downstream B-NN number is reportable** — the generator is the bottleneck.
Compare:   This is a per-environment control, not a candidate comparison. All candidates run their own B-00; if any B-00 disagrees by > 5% from the others, the host-invariant comparison is unsound and is rerun.

### B-01 — Throughput at small frame sizes, 1 vCPU
Reference: 02-packet-path.md § "Performance notes"; ADR-0013; RFC 2544 §9.1, §24, §26.1
Workload:  Unidirectional UDP, MLRsearch (NDR + PDR @ 0.001%) at **64 B and 128 B** frame sizes
Metric:    Mpps and Gbps at NDR and PDR; CPU utilisation
Pass:      thurward sustains its documented small-frame floor on 1 vCPU. The 1 Gbps headline target is **not** asserted at 64 B — the docs commit ~1 Gbps for *realistic mixed-size* traffic, not worst-case small frames. B-01 establishes the floor.
Compare:
  - nftables: expected within ~2x of thurward (similar single-thread softirq budget).
  - pf (OPNsense): typically below nftables on the same VM.
  - VyOS: indistinguishable from nftables (same kernel codepath).
  - VPP (ceiling): order of magnitude higher (10–30+ Mpps on dedicated DPDK hardware).

### B-02 — Throughput at mid frame sizes, 1 vCPU
Reference: 02-packet-path.md § "Performance notes"; RFC 2544 §9.1, §24, §26.1
Workload:  Unidirectional UDP, MLRsearch at **256, 512, and 1024 B**
Metric:    Gbps at NDR and PDR
Pass:      ≥ ~1 Gbps on 1 vCPU within thurward's stated envelope at 512 B and above.
Compare:   thurward ≈ nftables ≈ VyOS on the same VM; pf typically below; VPP ceiling 10x+.

### B-03 — Throughput at large frame sizes, 1 vCPU
Reference: 02-packet-path.md § "Performance notes"; RFC 2544 §9.1, §24, §26.1
Workload:  Unidirectional UDP, MLRsearch at **1280 and 1518 B**
Metric:    Gbps at NDR and PDR
Pass:      ≥ 1 Gbps comfortably; the easy case for all candidates.
Compare:   All apples-to-apples candidates expected to saturate the virtio link; differentiation is via CPU utilisation, not throughput.

### B-04 — Throughput at IMIX, 1 vCPU (bidirectional)
Reference: 02-packet-path.md § "Performance notes" ("realistic mixed-size traffic clears it comfortably"); RFC 9411 §7.x (bidirectional)
Workload:  RFC-2544 IMIX (7×64 B + 4×570 B + 1×1518 B per cycle), TRex stateful mode, **bidirectional traffic** (real firewalls peak ~2x higher unidirectional; bidirectional is the real envelope)
Metric:    Gbps at NDR
Pass:      **≥ 1 Gbps bidirectional on 1 vCPU** — this is the headline v1 envelope check.
Compare:   nftables, VyOS expected at or near thurward; pf typically a notch below; VPP ceiling much higher.

### B-1a — Frame-loss-rate curve
Reference: RFC 2544 §26.3
Workload:  Sweep offered load from 100% down in 10% steps at 64 B and IMIX; ≥ 60 s per step
Metric:    PLR per step → loss-vs-offered-load curve
Pass:      Curve is monotonic; no cliff before NDR; documented degradation past NDR. The *curve*, not a point, is what tells operators how the SUT behaves under bursty overload.
Compare:   Curves overlaid in the §8 supplementary material; thurward expected to degrade gracefully past NDR (no cliff) thanks to fixed-size batches and no dynamic allocator on the hot loop.

### B-3a — Back-to-back burst tolerance
Reference: RFC 2544 §26.4; RFC 9004; 02-packet-path.md (batched RX up to 64 frames)
Workload:  Bursts at minimum IFG, trial length ≥ 2 s, ≥ 50 repetitions per burst length
Metric:    Longest loss-free burst length per candidate
Pass:      thurward's burst tolerance ≥ the documented batch size (64 frames per RX poll). Directly stresses the batched-RX claim in chapter 02.
Compare:   Kernel firewalls' burst tolerance depends on the NIC ring size and softirq budget; VPP's depends on the DPDK ring size; thurward's is bounded by the documented 64-frame batch plus internal queueing.

### B-05 — Throughput scaling, 1 → 2 → 4 vCPUs
Reference: ADR-0013 (single-queue v1 ceiling; multi-queue v1.x stretch); 09-limitations.md § "Throughput at 10G+"
Workload:  Bidirectional IMIX at 1, 2, 4 vCPUs with corresponding virtio-net queue counts
Metric:    Throughput delta per added vCPU; throughput per vCPU
Pass:      v1 (single RX poll thread per interface): **flat curve past 1 vCPU**, by design. v1.x (multi-queue + per-thread conntrack shards): near-linear scaling to ~5 Gbps at 4 vCPUs.
Compare:   nftables/VyOS scale near-linearly with multi-queue today; pf has historically lagged in multi-thread scaling; VPP scales near-linearly by design.

---

## 2. Latency under load

*The hot path explicitly does not block on observability emit
(02-packet-path.md § "Performance notes"); this section verifies the
claim under realistic load.*

### B-06 — Latency at the throughput rate (RFC 2544 §26.2)
Reference: RFC 2544 §26.2
Workload:  TRex latency probes injected into IMIX traffic offered at the NDR established by B-04. Stream length ≥ 120 s, tag inserted at t = 60 s, **≥ 20 repetitions**.
Metric:    Average latency (RFC 2544 single-value), plus p50 / p99 / p99.9 from the trial-aggregated histogram
Pass:      thurward median ≤ a documented threshold (TBD by harness; typical edge target ≤ 100 µs p99 at NDR).
Compare:   nftables/VyOS expected close at the same VM; pf typically higher tail; VPP ceiling sub-µs p50.

### B-06a — Latency profile at 50% of throughput
Reference: 02-packet-path.md § "Performance notes" (post-decision emit; no allocator on hot loop)
Workload:  TRex one-way latency probes interleaved with IMIX at 50% of B-04 NDR
Metric:    p50 / p99 / p99.9 (µs)
Pass:      Lower tail than B-06 (load-dependent profile). Not an RFC-2544 metric; included for the operational curve.
Compare:   Same shape as B-06.

### B-07 — Tail latency at 90% of throughput
Reference: 02-packet-path.md § "Performance notes" (vsock backpressure does not gate data path)
Workload:  TRex latency probes at 90% of B-04 NDR; observability consumer simultaneously throttled to verify vsock-drop behaviour
Metric:    p99 / p99.9 latency and observed forwarding loss
Pass:      Tail degrades gracefully (no cliff). **Zero forwarding loss attributable to vsock backpressure** — oldest observability lines drop instead.
Compare:   nftables/VyOS exhibit kernel-softirq tail spikes; pf similar; VPP ceiling: flat tail until queue saturation.

### B-7a — Recovery after sustained overload
Reference: RFC 2544 §26.5
Workload:  110% of NDR offered for ≥ 60 s, then drop to 50% of NDR; measure time to first loss-free 1-s window
Metric:    Recovery time (s); behaviour during overload (drop discipline, no crashes)
Pass:      Recovery time bounded and documented; no crashes or stuck state. Distinct from B-20 crash recovery — this is overload recovery without a process death.
Compare:   Kernel firewalls typically recover in < 1 s; pf is in the same band; VPP designed to never enter loss state in this window.

---

## 3. Stateful conntrack scaling

*Default conntrack capacity 64 k entries (chapter 02). Validates that
the open-addressed hash stays O(1) amortised at scale, and characterises
behaviour past the documented ceiling.*

### B-08 — Steady-state throughput at concurrent-flow scale
Reference: 02-packet-path.md § "Performance notes" (open-addressed conntrack hash); FD.io CSIT NAT44 session-scale conventions
Workload:  TRex ASTF with **1 k / 10 k / 64 k / 128 k / 256 k** concurrent TCP flows, IMIX payload
Metric:    Gbps and CPU at each scale point
Pass:      No measurable throughput degradation between 1 k and 64 k flows on thurward (validates the O(1) amortised claim). At 128 k and 256 k, behaviour is documented: either thurward scales further by virtue of resizable conntrack, or 64 k is enforced as a hard ceiling with clean drop semantics (per S-15).
Compare:   nftables conntrack scales with `nf_conntrack_max`; pf slightly less efficient at high state-table fill; VPP n/a (bypasses kernel conntrack).

### B-09 — Conntrack-table-full behaviour
Reference: 02-packet-path.md; behavioural scenario [`scenarios.md` S-15](./scenarios.md#s-15--conntrack-table-full-new-flow-dropped-pressure-metric-increments)
Workload:  Saturate conntrack to 100% capacity, then continue sending new-flow SYNs while existing flows age out
Metric:    Drop rate on new flows, recovery time, `conntrack_pressure_drops` metric increments
Pass:      Drops are clean (no silent bypass, no crash). Recovery begins as soon as the oldest entries hit their documented timeouts.
Compare:   nftables drops cleanly to `nf_conntrack_max`; pf likewise. VPP n/a.

### B-9a — Throughput preserved under SYN flood
Reference: RFC 3511 §5.5 (legacy but still a fair question); 02-packet-path.md
Workload:  B-04 steady-state IMIX with a parallel SYN flood at increasing rates (1 k / 10 k / 100 k SYN/s) from a separate source pool
Metric:    Baseline IMIX throughput delta vs. flood rate; new-flow CPS available to legitimate traffic during the flood
Pass:      Baseline throughput retained within documented tolerance (≤ 5% drop) up to the documented SYN-flood rate. Past that rate, flood traffic is dropped (conntrack pressure) without compromising existing flows.
Compare:   nftables with `synproxy` performs well; pf likewise; VyOS inherits nftables; VPP ceiling: dedicated SYN-flood handling.

### B-10 — Max new TCP-flow rate (CPS)
Reference: RFC 9411 §7.2; ADR-0014 (conntrack install cost)
Workload:  TRex ASTF, ramp new TCP connections/sec until either drops appear or CPU saturates; sustain ≥ 300 s at the discovered rate
Metric:    Sustained CPS before drops appear
Pass:      thurward's CPS ≥ a documented realistic edge target (TBD by harness; typical SOHO edge: 5–10 k cps).
Compare:   nftables typically the strongest of the apples-to-apples set; pf historically lower; VyOS ≈ nftables; VPP ceiling much higher.

### B-10a — Max concurrent TCP-flow capacity
Reference: RFC 9411 §7.5; conntrack capacity documented in chapter 02
Workload:  TRex ASTF, ramp concurrent connections until conntrack pressure begins; hold at saturation for ≥ 300 s
Metric:    Concurrent-flow ceiling at which no new flows can install without eviction
Pass:      Matches the documented conntrack capacity (default 64 k). Behaviour past the ceiling is asserted by B-08 / B-09.
Compare:   nftables matches `nf_conntrack_max`; pf state-table ceiling; VPP n/a.

---

## 4. SNAT / DNAT under load

*Covers the NAT block in [`examples/rules.yaml:18-28`](../examples/rules.yaml)
and the translation rules in
[ADR 0014](../docs/architecture/decisions/0014-stateful-nat.md).*

### B-11 — SNAT throughput vs. plain forwarding
Reference: ADR-0014; examples/rules.yaml:18-22
Workload:  B-04 (bidirectional IMIX, 1 vCPU) with SNAT/masquerade enabled vs. disabled
Metric:    Throughput delta (% overhead of translation)
Pass:      SNAT overhead within a documented bound (TBD; typical kernel firewalls: 5–15%).
Compare:   nftables masquerade overhead is well-characterised at single-digit %; pf comparable; VyOS ≈ nftables; VPP ceiling: NAT is its strength.

### B-12 — SNAT port-pool exhaustion
Reference: ADR-0014; 09-limitations.md § "CGNAT-scale port allocation"; behavioural scenario [`scenarios.md` S-19](./scenarios.md#s-19--snat-port-pool-exhaustion-drops-new-flows)
Workload:  Force per-external-IP port pool exhaustion by ramping concurrent flows from a single internal source
Metric:    Threshold (flow count) at which `snat_pool_exhaustion` begins incrementing; behaviour past that point
Pass:      New flows drop cleanly past the threshold; existing flows continue. Threshold matches the documented pool size for the SNAT external IP.
Compare:   nftables/pf/VyOS also hit a per-external-IP port-pool ceiling at ~64 k; VPP ceiling: explicit CGNAT mode pushes the ceiling higher.

### B-13 — DNAT throughput
Reference: ADR-0014; examples/rules.yaml:23-28
Workload:  B-04 but with all traffic ingress-DNAT'd via the worked DNAT entry; **return path measured separately**
Metric:    Forward and return throughput vs. plain forwarding
Pass:      DNAT overhead symmetric to SNAT (single hash lookup + rewrite). Return-path throughput within documented tolerance of forward-path.
Compare:   All apples-to-apples candidates similar; VPP ceiling much higher.

---

## 5. Rule-count sensitivity (log scale)

*Validates the [02 — Packet path](../docs/architecture/02-packet-path.md)
§ "Performance notes" claim that linear scan is fine at < 200 rules,
characterises the cliff past the envelope, and informs the deferred
CIDR-trie optimisation. Log-scale sweep pattern adopted from
the IncludeOS Sci. Reports 2024 study.*

### B-14 — Rule-count throughput sweep
Reference: 02-packet-path.md § "Performance notes"; 03-rule-model.md § "Build-time compilation"; IncludeOS Sci. Reports 2024 sweep pattern
Workload:  Bidirectional IMIX, 1 vCPU, rule-table sizes **1 / 10 / 100 / 1 k / 10 k / 100 k / 1 M** rules. Three traffic-distribution variants per size:
  - **B-14a (best case)** — every packet matches rule #1
  - **B-14b (worst case)** — every packet falls through to the last rule (or to default-deny on a 1 M-rule table)
  - **B-14c (uniform-random)** — packets distributed uniformly across rule positions
Metric:    Throughput at NDR per (rule count × variant)
Pass:      At ≤ 200 rules (v1 envelope per 02-packet-path.md), worst-case throughput within a documented bound of best-case (target: ≤ 10% degradation under uniform-random). Past 200, performance is *characterised*, not asserted — informs the CIDR-trie ADR.
Compare:   nftables uses verdict-map trees and degrades gracefully across the sweep; pf uses interpreted ruleset and degrades faster; VyOS ≈ nftables; VPP classify ACLs are tree-based; expected near-flat across the full sweep.

### B-15 — Outside-envelope cliff analysis
Reference: 02-packet-path.md § "Performance notes" ("CIDR trie is a known deferred optimisation"); B-14 data
Workload:  Interpret the B-14 sweep
Metric:    Rule count at which thurward drops below 50% of its 10-rule throughput under uniform-random distribution
Pass:      Documents the operational ceiling where the linear-scan model breaks down. **Informs whether a CIDR-trie ADR is warranted for v1.x.**
Compare:   nftables/pf/VyOS expected to maintain ≥ 50% well past 1 M thanks to structured matching; VPP near-flat.

---

## 6. FQDN rule and DNS proxy performance

*Covers the data plane's FQDN cache lookup
([04 — FQDN & DNS](../docs/architecture/04-fqdn-and-dns.md)
§ "fqdn_set cache") and the proxy's QPS ceiling.*

### B-16 — Throughput with 100% FQDN-rule traffic (warm cache)
Reference: 02-packet-path.md § "Performance notes" ("fqdn_set: dst-IP-keyed hash, O(1) lookup"); 04-fqdn-and-dns.md
Workload:  Bidirectional IMIX, 1 vCPU; rule set forces every egress packet to be evaluated against an FQDN rule. **FQDN cache pre-warmed before measurement begins (ramp-up phase per §0.5).**
Metric:    Throughput vs. an equivalent rule set with no FQDN rules
Pass:      FQDN lookup overhead < 10% vs. the no-FQDN baseline (validates the O(1) hash claim under realistic load).
Compare:   nftables FQDN-equivalents are typically set-based with periodic resolution and similar cost; pf uses table reloads; VyOS ≈ nftables; VPP has no native FQDN matching (n/a). **Cold-cache behaviour is measured separately in B-18.**

### B-17 — DNS proxy QPS ceiling and response latency
Reference: 04-fqdn-and-dns.md § "Per-client rate-limiting"; examples/rules.yaml:10 (`dns_qps_per_client: 100`)
Workload:  Synthetic DNS load. Two configurations:
  - Single client ramped from 50 to 200 QPS (validates per-client limit)
  - N clients each at 100 QPS (finds aggregate ceiling)
Metric:    Per-client limit kicks in exactly at `dns_qps_per_client`; aggregate ceiling QPS; **p50 and p99 DNS response latency at each QPS step**
Pass:      Per-client SERVFAIL behaviour matches [`scenarios.md` S-32](./scenarios.md#s-32--per-client-dns-qps-limit). Latency p99 stays below a documented threshold (TBD; typical: ≤ 10 ms p99 from local proxy) up to the documented aggregate ceiling.
Compare:   Most peers don't ship a built-in DNS proxy; nftables/pf/VyOS need an external resolver (Unbound, dnsmasq) with its own QPS curve. VPP n/a.

### B-18 — FQDN cache cold-start warm-up
Reference: 09-limitations.md § "Restart loses fqdn_set"; 04-fqdn-and-dns.md § "Pitfalls — Restart loses the cache"
Workload:  Restart thurward; measure FQDN-rule miss rate over the first N seconds as clients re-resolve. Distinct from B-16 (warm-cache).
Metric:    Miss-rate-vs-time curve; time-to-99%-cache-hit-rate
Pass:      Curve monotonically improving; no pathological churn (entries don't expire faster than they're inserted). Informs the design of the optional `fqdn_set_warm` replay mechanism documented in chapter 07.
Compare:   Peers with external resolvers (Unbound) cold-start similarly; n/a for plain stateful firewalls without FQDN semantics.

---

## 7. Resilience and recovery

*Covers single-VM crash-and-restart behaviour from
[09 — Limitations](../docs/architecture/09-limitations.md) § "No high availability" and the launcher restart described in
[06 — Deployment](../docs/architecture/06-deployment.md). B-23 (launch time) lives in section 8 because it's an IncludeOS-style metric, but it overlaps with the resilience concern.*

### B-19 — Boot time to first forwarded packet (legacy second-resolution)
Reference: 06-deployment.md (`Restart=always` launcher); 09-limitations.md § "No high availability"
Workload:  Cold launcher restart with a TRex probe stream already running through the bridge
Metric:    Wallclock from process start to first packet forwarded, at second resolution
Pass:      thurward boots in **single-digit seconds** (Hermit unikernel boot dominates; no userspace init). **Superseded for precision work by B-23 (sub-second resolution).**
Compare:   nftables-on-Linux: ~1–2 s once kernel is up; pf-on-FreeBSD-VM: 10 s+ for full VM boot; VyOS: 20 s+; VPP: daemon already running (n/a).

### B-20 — Recovery after process crash
Reference: 09-limitations.md § "No high availability" ("default deny holds; every NAT binding is lost")
Workload:  Kill the thurward process under steady IMIX load; measure time until forwarding resumes and the first new flow completes a handshake. Wallclock-measured, not subject to the §0.5 lifecycle.
Metric:    Wallclock from kill to resumed forwarding; count of dropped flows during the outage
Pass:      Restart-and-resume bounded (single-digit seconds, dominated by Hermit boot). All in-flight NAT bindings are lost — documented behaviour, asserted by counting forced reconnects.
Compare:   nftables / VyOS: process is the kernel, "crash" means VM reboot (much slower); pf likewise; VPP: daemon restart is in thurward's order of magnitude.

---

## 8. Substrate / footprint metrics (IncludeOS-inspired)

*The IncludeOS Sci. Reports 2024 paper (Bratterud et al.,
<https://www.nature.com/articles/s41598-024-51167-8>) established the
expectation that a unikernel-vs-kernel firewall comparison reports image
size, launch time, and idle latency alongside throughput. This section
adopts those metrics with the methodological rigour the paper itself
flagged as future work (median + p95, not averages without spread).*

### B-21 — Image size
Reference: IncludeOS Sci. Reports 2024 (image sizes: IncludeOS 9.3–30 MB vs. KVM-Ubuntu 1829–1916 MB)
Workload:  Inspection of the signed release artifact per candidate at three rule-table sizes: 10, 1 k, and 100 k rules
Metric:    Image size in MB
Pass:      thurward target band: **IncludeOS class (single-digit-to-tens MB)**, dominated by the compiled rule table at large counts. Image-size delta from 10 → 100 k rules documents the per-rule overhead.
Compare:
  - nftables-on-Linux-VM: ~1.5–2 GB (full distro)
  - OPNsense: ~500 MB minimal install
  - VyOS: ~300 MB minimal install
  - VPP: small core (~200 MB) on a thin Linux base
  - Note: image size is also an **attack-surface proxy** — smaller = fewer unrelated binaries on the box.

### B-22 — Memory footprint
Reference: IncludeOS paper § "future work" (RSS not measured); closes that gap
Workload:  Four measurement points per candidate:
  1. Boot, idle, conntrack empty
  2. B-04 saturation, conntrack ~10% full
  3. Conntrack at 64 k entries
  4. Conntrack at 256 k entries (where supported)
Metric:    RSS at each point (MB), reported as median across ≥ 10 trials
Pass:      thurward RSS at idle ≤ a documented bound (TBD; target: tens of MB). Growth from idle → 64 k flows linear and bounded.
Compare:   Kernel firewalls' RSS is the whole VM's memory — they don't have a separate firewall-process RSS in the same sense. Reported as "VM memory at idle" vs. thurward's unikernel total.

### B-23 — Launch time (sub-second resolution)
Reference: IncludeOS paper § "launch time" (5 168–5 840 ms); supersedes B-19 for precision work
Workload:  Cold launcher restart; ≥ 100 repetitions (IncludeOS convention); measurement spans `systemctl start` return to first egress packet timestamped at the TX-side tap
Metric:    Median + p95 of launch time in milliseconds
Pass:      Target: **comparable to or better than IncludeOS' ~5 s** for a fully-loaded firewall image. Faster reflects Hermit's faster boot path or a leaner init sequence.
Compare:   Per IncludeOS paper: KVM-Ubuntu ~13 500 ms; Docker ~1 650 ms; LXD ~1 235 ms — those are full-OS baselines, not directly comparable to a unikernel cold boot but documented for context.

### B-24 — Idle latency baseline
Reference: IncludeOS paper § "idle ping delay" (100 ICMP at 1 s interval)
Workload:  100 ICMP echo at 1 s interval through an idle DUT; tooling: `hping3` or equivalent for compatibility with the IncludeOS-paper baseline
Metric:    Median and p95 ICMP echo RTT (ms)
Pass:      Idle RTT comparable to the no-firewall baseline (the host-bridge-host path with no DUT in line). Outlier vs. the no-firewall baseline indicates host-side variance and **invalidates the run** — used as a sanity check, not a headline number.
Compare:   All candidates expected within tight bounds of the no-firewall baseline at idle. Differentiation is in B-06/B-06a/B-07 (latency under load).

---

## 9. Comparison summary

This is the **operator-facing one-page answer** to "how does thurward stack up?". The harness fills the cells; this file defines the columns. Each cell reports **median + p95 across N ≥ 5 trials** unless otherwise stated (§0.11). VPP rows that are non-substrate-sensitive (boot, recovery) are measured; throughput rows are ceiling references (footnoted with CSIT release / NIC / CPU).

| Benchmark                | thurward (v1)         | nftables / KVM | pf / OPNsense | VyOS | VPP (ceiling)    | Thurward Gbps / vCPU |
|--------------------------|-----------------------|----------------|---------------|------|------------------|----------------------|
| B-00 ip4base 64 B Mpps   | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-01 64/128 B Mpps       | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-02 256/512/1024 B Gbps | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-03 1280/1518 B Gbps    | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-04 IMIX bidir Gbps     | **≥ 1.0 (target)**    | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-1a FLR curve shape     | graceful (target)     | TBD            | TBD           | TBD  | flat             | n/a                  |
| B-3a burst frames        | ≥ 64 (target)         | TBD            | TBD           | TBD  | ring-bounded     | n/a                  |
| B-05 4-vCPU Gbps         | flat (v1) / ~5 (v1.x) | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-06 p99 @ NDR µs        | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | n/a                  |
| B-06a p99 @ 50% µs       | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | n/a                  |
| B-07 p99.9 @ 90% µs      | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | n/a                  |
| B-7a overload recovery s | TBD                   | TBD            | TBD           | TBD  | flat             | n/a                  |
| B-08 256k flows Gbps     | TBD                   | TBD            | TBD           | TBD  | n/a              | TBD                  |
| B-09 table-full          | clean drop            | clean drop     | clean drop    | clean drop | n/a       | n/a                  |
| B-9a SYN flood retained  | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | n/a                  |
| B-10 CPS                 | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-10a concurrent flows   | 64 k (default)        | `nf_conntrack_max` | state table | `nf_conntrack_max` | n/a | n/a               |
| B-11 SNAT overhead %     | TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | n/a                  |
| B-12 SNAT pool ceiling   | TBD                   | TBD            | TBD           | TBD  | CGNAT mode       | n/a                  |
| B-13 DNAT fwd/return Gbps| TBD                   | TBD            | TBD           | TBD  | ceiling [^vpp]   | TBD                  |
| B-14 worst-case 200 rules| ≤ 10% (target)        | TBD            | TBD           | TBD  | flat             | n/a                  |
| B-15 cliff rule count    | TBD                   | TBD            | TBD           | TBD  | n/a (flat)       | n/a                  |
| B-16 FQDN warm overhead %| ≤ 10% (target)        | needs sidecar  | needs sidecar | needs sidecar | n/a       | n/a                  |
| B-17 DNS QPS / p99 ms    | TBD                   | external       | external      | external | n/a          | n/a                  |
| B-18 FQDN warm-up s      | TBD                   | n/a            | n/a           | n/a  | n/a              | n/a                  |
| B-19 boot s              | TBD (single-digit)    | ~1–2           | 10+           | 20+  | daemon (n/a)     | n/a                  |
| B-20 recovery s          | TBD (single-digit)    | reboot         | reboot        | reboot | daemon restart | n/a                  |
| B-21 image size MB       | tens (target)         | ~1500–2000     | ~500          | ~300 | ~200             | n/a                  |
| B-22 idle RSS MB         | tens (target)         | full VM        | full VM       | full VM | daemon        | n/a                  |
| B-23 launch ms (p95)     | TBD (target ≤ 5000)   | ~13 500        | ~10 000+      | ~20 000 | daemon (n/a)  | n/a                  |
| B-24 idle ICMP ms        | ≈ no-fw baseline      | ≈ baseline     | ≈ baseline    | ≈ baseline | ≈ baseline  | n/a                  |

[^vpp]: VPP "ceiling" numbers reference the latest FD.io CSIT report at <https://docs.fd.io/csit/master/report/>. Each ceiling cell records the CSIT release tag, NIC model (typically Intel E810 or Mellanox CX6), and CPU SKU so the comparison is auditable rather than folkloric.

---

## 10. Cross-cutting verification notes

- Every `Reference:` line above resolves to a real RFC section, ADR, architecture chapter, `examples/rules.yaml` line, or scenario in `scenarios.md`. A `grep` over the cited locations confirms each citation.
- Every v1 limitation that excludes a workload (IPv6, fragments, hairpin NAT, ALG, HA, ARM, bare-metal) is **explicitly excluded** in §0.13, not silently missing.
- Every apples-to-apples candidate (nftables, pf, VyOS) appears in every `Compare:` row where the workload is meaningful. **VPP** is consistently marked as a ceiling reference; rows where it doesn't apply (B-08 256k, B-09, B-16, B-17, B-18) say so with a stated reason.
- Every `Pass:` criterion is tied to thurward's own envelope from the architecture docs — not to a peer's number. Comparison rows describe **expected behaviour**, not predicted numbers.
- **Security-effectiveness precondition (§0.8)** is enforced: any image that fails `scenarios.md` is not eligible for §9. A fast-but-broken firewall is not benchmarked.
- The IncludeOS Sci. Reports 2024 paper measured 10 metrics; this spec adopts 8 of them (image size, launch time, idle ICMP latency, TCP/UDP throughput, TCP/UDP CPS, TCP latency). The two not adopted are iperf-only TCP/UDP throughput numbers (superseded by MLRsearch via TRex, which is harder to game) and IncludeOS' single-host chain (replaced by separated-host topology per §0.3). Each non-adoption is a deliberate methodological upgrade, not an oversight.
- Once a data plane exists, each B-NN above becomes a harness-driven benchmark, the TBD cells in §9 fill in, and this file is the contract the harness implements.
