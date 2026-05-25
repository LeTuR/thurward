# `bench/results/SCHEMA.md` — JSON envelope for every harness result

Every benchmark run (smoke or B-NN) writes one `result.json` file under
`bench/results/<run-type>/<UTC-timestamp>/`. The format below is the
*minimum* envelope required by `tests/benchmarks.md` § 0.11 reporting
discipline. Per-benchmark fields are added under `metric` without
removing or renaming the envelope fields.

## Envelope

```jsonc
{
  "schema_version": 1,

  // smoke | b-00 | b-01 | b-04 | b-23 | …  matches tests/benchmarks.md B-NN.
  "result_type": "smoke",

  // The candidate exercised. Must be one of:
  //   thurward, nftables, pf, vyos, vpp
  // (more may join as the harness grows; pf and vyos and vpp are Tier-1).
  "candidate": "nftables",

  // Where the harness ran. See bench/ROADMAP.md for the tier definitions.
  //   0-workstation-virtio
  //   1-lab-sriov-rt
  //   2-ci-self-hosted
  "tier": "0-workstation-virtio",

  // ISO-8601 UTC, populated by the harness.
  "timestamp_utc": "2026-05-25T15:55:14+00:00",

  // BMWG containerised-infra invariants per tests/benchmarks.md § 0.4.
  // The harness records what was actually used; the comparison rules
  // enforce equality across candidates.
  "host_invariants": {
    "kernel": "Linux 7.0.9-arch1-1",
    "cpu_model": "Intel(R) Core(TM) i7-…",
    "numa_pinning": "none | core-0-3 | …",
    "hugepages": "none | 1024x2M | …",
    "nic": "virtio-net | e810-sriov | …",
    "queue_count": 1
  },

  // RFC-9411 § 0.5 lifecycle — populated by long-running benchmarks
  // (B-04+). Smoke results may omit it.
  "trial_lifecycle": {
    "init_s": 5,
    "ramp_up_s": 30,
    "sustain_s": 300,
    "ramp_down_s": 10,
    "sample_interval_s": 1
  },

  // RFC-9411 § 0.8 — every perf number from this image must be paired
  // with a passing scenarios.md run on the same image. Smoke runs
  // record the in-line precondition; B-NN runs reference an external
  // scenarios.md result file.
  "security_effectiveness_precondition": {
    "description": "…",
    "satisfied": true,
    "scenarios_md_run_ref": null
  },

  // Per-result-type payload. The harness reports MEDIAN + P95 across N
  // ≥ 5 trials (§ 0.11). Latency uses ≥ 20 trials per RFC 2544 § 26.2.
  "metric": {
    "name": "throughput_bps_imix_bidir",
    "median": 950000000,
    "p95": 970000000,
    "trials": 5,
    "raw_samples_file": "raw.log"
  },

  // Free-form notes. The harness should at least flag known caveats
  // (Tier-0 virtio ceiling, etc.).
  "notes": ["…"]
}
```

## Hard rules (the schema validator will reject otherwise)

1. **Every result MUST set `tier`**. A Tier-0 number tagged as Tier-1
   is methodologically corrupt and gets the result thrown out.
2. **Every result MUST set `security_effectiveness_precondition.satisfied`**.
   If `false`, the result is *recorded* but never included in the §9
   comparison table.
3. **`host_invariants` is required for every B-NN result**. Smoke
   results may stub `numa_pinning` and `hugepages` to `"none"` but the
   keys must exist.
4. **N (trials count) MUST be present** for any non-smoke result.

## Why this exists separately from the benchmarks.md spec

`tests/benchmarks.md` is the **what** — the workload, the metric, the
pass criterion, the standards. This file is the **format** that the
harness writes so the §9 comparison table can be assembled
mechanically and audited later. Keeping the format under the harness
keeps the spec stable while the result schema can iterate.
