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
