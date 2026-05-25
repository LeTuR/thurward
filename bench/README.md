# `bench/` — QEMU/KVM benchmark harness

*Implements the methodology defined in
[`tests/benchmarks.md`](../tests/benchmarks.md) § 0.*

This directory holds the **black-box benchmark harness** that runs the
`B-NN` benchmarks defined in `tests/benchmarks.md` against
thurward and four comparison candidates (Linux nftables, OPNsense/pf,
VyOS, VPP/FD.io).

## What's shipped today (Tier 0)

This is the **first slice** of the harness. It can:

- Bring up two host-internal libvirt bridges (`lan-thurward`, `wan-thurward`).
- Boot a nftables firewall SUT VM (Debian 12 + cloud-init + a structured
  nftables ruleset translated from [`examples/rules.yaml`](../examples/rules.yaml)
  per `tests/benchmarks.md` § 0.7).
- Boot a traffic-generator VM with two NICs (one on each bridge) running
  `iperf3` + `ping` for reachability smoke tests.
- Run a smoke test that records:
  - that allowed traffic crosses the firewall
  - that denied traffic is correctly dropped
    (satisfies `tests/benchmarks.md` § 0.8 security-effectiveness precondition)
  - rough iperf3 TCP throughput

## What's coming next (Tier 0 → Tier 1)

Deferred to subsequent passes:

- The other three candidates (pf/OPNsense, VyOS, VPP/DPDK).
- TRex generator VM with MLRsearch NDR/PDR (replaces iperf3 for real
  RFC-9411-compliant measurements).
- FQDN-rule translation strategy for non-thurward candidates (sidecar
  resolver + ipset feeder; merits its own ADR).
- Image-size, RSS, launch-time, idle-ICMP metrics (B-21..B-24) — these
  require a thurward image, which doesn't exist yet.

## Tier-0 trade-offs you should know

The harness runs on a single workstation with virtio-net NICs.
That's good enough to characterise behaviour against thurward's
~1 Gbps v1 envelope; it is **not** good enough to compete with VPP's
10+ Mpps ceiling on physical-NIC line-rate. Concretely:

1. **Virtio-net is the throughput ceiling**, not the SUT. Tier-0
   numbers are valid for SUTs in thurward's envelope; numbers at or
   above ~2 Gbps may be reporting the virtio path rather than the
   firewall.
2. **No NUMA isolation control beyond CPU pinning** — the host kernel
   continues to run on the same node as the VMs.
3. **Hugepages allocated but not isolated** — host kernel still owns
   the rest of memory.
4. **Real-time scheduling not enforced** — sustain-phase tail latency
   includes host noise.

Tier 1 — physical-NIC SR-IOV + RT kernel + dedicated NUMA node — is
documented in [`ROADMAP.md`](./ROADMAP.md) and will land when the
harness graduates from this workstation onto a dedicated lab box.

## Layout

```
bench/
├── README.md          ← this file
├── SETUP.md           ← Arch package install, libvirt config
├── ROADMAP.md         ← Tier 0/1/2 progression
├── Makefile           ← orchestration
├── topology/
│   ├── lan-thurward.xml   ← libvirt network XML for LAN bridge
│   └── wan-thurward.xml   ← libvirt network XML for WAN bridge
├── candidates/
│   ├── nftables/      ← Debian + nftables SUT
│   │   ├── cloud-init/
│   │   ├── rules.nft
│   │   └── Makefile.inc
│   └── trafficgen/    ← Debian + iperf3 generator
│       └── cloud-init/
├── results/
│   ├── SCHEMA.md      ← JSON envelope every result must populate
│   └── .gitignore     ← exclude per-run output
├── scripts/           ← helper shell scripts
├── images/            ← cached base images (gitignored)
└── keys/              ← generated harness SSH keys (gitignored)
```

## Quick start

After running the steps in [`SETUP.md`](./SETUP.md) once on a fresh box:

```
make net-up                       # bring up the two bridges
make sub-up CAND=nftables         # boot the nftables firewall SUT
make gen-up                       # boot the traffic generator
make smoke CAND=nftables          # run the reachability smoke test
make tear-down                    # destroy everything
```

Results land under `bench/results/smoke/<UTC-date>/` as JSON.

## Scope decision: throughput is Tier-1 only

The bench tree contains a TRex generator + sink-VM scaffolding
(`make trex-up`, `make sink-up`, `make b-00`) but **headline throughput
numbers (B-00 through B-13) are deliberately out of scope on Tier 0**.

Reasoning:

- The host is a developer workstation. The SUT VM has 1 vCPU; the
  virtio-net path between qemu processes caps well below TRex's
  software-mode ceiling, which itself caps well below TRex's normal
  DPDK-NIC ceiling. **Any throughput number produced here measures the
  virtio path, not the SUT's envelope.**
- `tests/benchmarks.md` § 0.6 already says B-00 is a per-environment
  control: if the control itself is loss-limited, downstream B-NN
  numbers are not reportable. On this Tier-0 setup the control is
  always loss-limited.
- The right time to run B-00..B-13 is on the Tier-1 lab box
  ([`ROADMAP.md`](./ROADMAP.md)) — physical NIC + SR-IOV + RT kernel +
  dedicated NUMA node. Until that hardware exists, those benchmarks
  are intentionally skipped, not faked.

What stays in scope on Tier 0:

- **Smoke** (`make smoke`) — reachability + ruleset-loaded precondition
  (per `tests/benchmarks.md` § 0.8).
- **Functional correctness** — rule translation discipline (§ 0.7),
  scenarios.md, security-effectiveness checks.
- **Footprint metrics** (B-21 image size, B-22 RSS, B-23 launch time,
  B-24 idle latency) — these are workstation-friendly and produce
  per-candidate numbers that don't depend on throughput.

The TRex/sink scaffolding stays in the tree so the Tier-1 box can run
B-00..B-13 immediately when it arrives — no harness rewrite needed.

## Latest Tier-0 smoke run

Run on 2026-05-25 against the `nftables` candidate on an Arch
workstation (Intel i7-8700K @ 3.7 GHz, virtio-net, 1 vCPU SUT).
Verdict: ✅ `OK` — precondition (`§ 0.8`) satisfied.

| Probe                                       | Result            | Interpretation                                                |
| ------------------------------------------- | ----------------- | ------------------------------------------------------------- |
| SUT `nftables-thurward.service`             | `active`          | Ruleset loaded; security-effectiveness precondition met       |
| ICMP gen-LAN → SUT-LAN (`10.10.0.1`)        | reachable         | `input` chain permits ICMP on `enp1s0` as designed            |
| ICMP gen-LAN → WAN (`203.0.113.50`)         | dropped (correct) | `forward` chain default-deny working                          |
| TCP/443 gen-LAN → WAN                       | crossed firewall  | `allow-github-https` rule active                              |
| iperf3 gen-LAN → gen-WAN-netns through SUT  | **~14 Gbps**      | TCP/5201 allowed; virtio-bounded (see Tier-0 caveat below)    |

**Tier-0 caveat.** ~14 Gbps is the virtio-net path between two qemu
processes on the same host — not the SUT envelope. See the "Scope
decision" section above and [`ROADMAP.md`](./ROADMAP.md) Tier-1 for
where comparable-to-vendor numbers will come from.

Run-specific result JSON lands under `results/smoke/<UTC-date>/` (not
in git — the per-run output is gitignored).

## Methodology pointer

Every harness run **must** satisfy the methodology in
[`tests/benchmarks.md`](../tests/benchmarks.md) § 0 — in particular:

- § 0.4 host invariants are recorded with every result
- § 0.7 translation discipline is followed (idiomatic structured forms,
  not flat rule lists)
- § 0.8 security-effectiveness precondition is enforced (a failing
  scenarios.md run invalidates the perf numbers from that image)
- § 0.11 reporting discipline (median + p95, ≥ 5 trials, ≥ 20 trials
  for latency)

If the harness ever produces a number that violates one of those,
the number is invalid; fix the harness, not the methodology.
