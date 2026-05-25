# `bench/ROADMAP.md` — Tier 0 → Tier 1 → Tier 2

The benchmark harness in this directory is intentionally staged. Each
tier is a separate set of trade-offs and infrastructure assumptions.

## Tier 0 — single workstation, virtio-net (shipping now)

**Goal:** prove the methodology, the topology, and the rule-translation
discipline on a developer workstation. Numbers are valid for
characterising behaviour in thurward's ~1 Gbps v1 envelope.

**Substrate**

- Single host (developer workstation)
- KVM + libvirt + virtio-net
- Host-internal libvirt bridges, no physical NICs
- iperf3 + ping for smoke; TRex (in a VM) for headline benchmarks

**What it can measure**

- Functional correctness (security-effectiveness precondition,
  `tests/benchmarks.md` § 0.8 — does the firewall actually enforce its
  rules?)
- Throughput up to ~1–2 Gbps with caveats
- Latency on the order of 100s of µs (host scheduler noise included)
- Conntrack / NAT correctness at moderate scale

**What it can't measure**

- VPP's 10+ Mpps DPDK ceiling
- Sub-microsecond latency
- Sustained PPS at small frames against physical-NIC line rate

**Candidates shipped this tier:** nftables (first), then pf/OPNsense,
VyOS, VPP/DPDK (each in its own PR).

## Tier 1 — dedicated lab box, physical NIC, SR-IOV

**Goal:** comparable RFC 9411 numbers against published vendor / FD.io
CSIT reports.

**Substrate**

- Dedicated lab box (not the developer workstation)
- Intel E810 or Mellanox CX-6 (or equivalent SR-IOV-capable NIC)
- TRex bound to physical NIC ports via SR-IOV virtual functions
- RT kernel patches (`linux-rt`) on host
- Dedicated NUMA node for the SUT VM and TRex VFs
- Hugepages isolated (`isolcpus`, `nohz_full`, `rcu_nocbs`)
- Separate network for management; no traffic on the bench wires

**What it adds**

- Honest virtio-vs-line-rate comparison
- Loss-free Mpps at 64 B (B-01 worst case)
- Frame-loss-rate curve (B-1a) at line rate
- Sub-microsecond latency floor

**When this lands:** when there's a dedicated box to ship the harness
on. Until then, Tier-0 numbers are tagged "workstation-virtio" in the
result envelope so they're never confused with Tier-1 measurements.

## Tier 2 — CI integration

**Goal:** every PR to thurward that touches the data plane runs a
subset of B-NN automatically and posts a comment with the delta vs
`main`.

**Substrate**

- Self-hosted GitHub Actions runner on the Tier-1 lab box
- Trigger: `labels: bench/run`
- Reports a compact summary of regressed benchmarks back to the PR

**When this lands:** after Tier-1 stabilises and `src/` exists.
Tier-2 is for *catching regressions*, not establishing the envelope —
the envelope still comes from Tier-1 runs.

## How tiers are tagged in results

Every result JSON (per `results/SCHEMA.md`) records its tier:

```
"tier": "0-workstation-virtio"
"tier": "1-lab-srlov-rt"
"tier": "2-ci-self-hosted"
```

Operators reading the §9 comparison table in `tests/benchmarks.md` can
filter by tier — Tier-0 numbers are for direction, Tier-1 for
publication, Tier-2 for regression-detection.
