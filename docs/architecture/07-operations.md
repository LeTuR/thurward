# 07 — Operations

*Audience: anyone running thurward in production. Alerts, dashboards,
runbooks.*

The operational shape is dictated by:

- [ADR 0005](decisions/0005-build-time-rule-compilation.md) — every
  rule change is a new artifact; therefore every change is a
  build-and-replace, not a hot patch.
- [ADR 0010](decisions/0010-supply-chain-hardening.md) — images ship
  with cosign signatures and SLSA-3 provenance; operators verify
  before installing.
- [ADR 0016](decisions/0016-image-per-change-user-deploy.md) — the
  operator (or their config-management) drives install and restart.
  There is no deploy controller.

## Applying a rule change

The full sequence for a rule change in production (Path B /
Firecracker from [06 — Deployment](06-deployment.md)):

1. Edit `rules.yaml` in your fork of the firewall repo. Open a PR if
   your workflow requires review; merge when approved.
2. CI builds the image, runs `cosign sign`, attaches the SLSA-3
   attestation, and publishes the artifact + signature + provenance
   to your registry / release page (see
   [ADR 0010](decisions/0010-supply-chain-hardening.md)).
3. On the target host, fetch the new artifact + signature + provenance.
4. Run `cosign verify` and `slsa-verifier verify-artifact` against the
   trusted key and source coordinates. Failure here is a hard stop —
   do not install.
5. Replace `/var/lib/thurward/current.image` and
   `systemctl restart thurward`.
6. Watch metrics for a minute. Confirm `thurward_build_info` reflects
   the new `git_sha` / `rules_sha` and that drop rates settle.

During the restart, **all in-progress flows drop** (there is no HA in
v1). Clients reconnect; default-deny holds for whatever brief window
the restart takes. This is the documented v1 trade-off
([09 — Limitations](09-limitations.md)).

## Recommended Prometheus alerts

```yaml
groups:
  - name: thurward
    interval: 30s
    rules:
      - alert: ThurwardDown
        expr: up{job="thurward"} == 0
        for: 2m
        labels: { severity: critical }
        annotations:
          summary: "thurward target {{ $labels.instance }} is down"

      - alert: ThurwardDropSpike
        expr: |
          sum by (rule_id) (rate(thurward_rule_drops_total[5m]))
            > 10 * sum by (rule_id) (rate(thurward_rule_drops_total[1h] offset 1h))
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "Drop rate on rule {{ $labels.rule_id }} 10× baseline"

      - alert: ThurwardDnsErrors
        expr: rate(thurward_dns_queries_total{outcome=~"servfail|rate_limited"}[5m]) > 5
        for: 10m
        labels: { severity: warning }

      - alert: ThurwardFqdnEvictionsHigh
        expr: rate(thurward_fqdn_set_evictions_total{reason="capacity_pressure"}[5m]) > 1
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "fqdn_set evicting under capacity pressure — bump cap or investigate CDN churn"

      - alert: ThurwardVsockDrops
        expr: rate(thurward_vsock_dropped_lines_total[5m]) > 0
        for: 1m
        labels: { severity: warning }
        annotations:
          summary: "Observability lines being dropped — collector slow or down"

      - alert: ThurwardConntrackPressure
        expr: thurward_conntrack_entries / on(instance) thurward_conntrack_capacity > 0.85
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "Conntrack table {{ $value | humanizePercentage }} full"

      - alert: ThurwardNatPortPoolPressure
        expr: thurward_nat_port_pool_in_use / 45000 > 0.85
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "SNAT port pool {{ $value | humanizePercentage }} used on {{ $labels.external_ip }}"
```

HA-specific alerts (`ThurwardStandbyMissing`, `ThurwardHaReplicationLag`)
that appeared in earlier drafts of this chapter are removed; HA is
deferred to v2 ([ADR 0015](decisions/0015-active-passive-ha.md)).

## Recommended Grafana dashboard panels

A `thurward — Overview` dashboard should contain at minimum:

- **Target health** — single stat from `up{job="thurward"}`.
- **Throughput** — pkts/sec accepted vs dropped, stacked area.
- **Per-rule activity** — table sorted by `rate(thurward_rule_drops_total)`
  + `rate(thurward_rule_accepts_total)`.
- **Drop reasons** — pie chart of `thurward_drops_total` by `drop_reason`.
- **Top dropped FQDNs** — LogQL over Loki, top-10 of
  `destination.domain` where `event.action="drop"`.
- **DNS proxy** — query rate by outcome, lookup-duration p50/p95/p99,
  `fqdn_set_size`, evictions/sec.
- **Conntrack/NAT** — `thurward_conntrack_entries`, NAT port-pool
  utilization, translations/sec.
- **Vsock health** — send queue depth, dropped lines/sec.
- **Build identity** — `thurward_build_info` as a single-stat showing
  `version`, `git_sha`, `rules_sha` — so on-call always knows what's
  running.

## Runbooks

### Incident: thurward target down

1. The VM crashed, or the launcher failed. Default deny holds; LAN
   clients see all flows fail. There is no automated failover in v1.
2. For Path B (systemd-managed): check `systemctl status thurward`
   and `journalctl -u thurward`. The reference unit has
   `Restart=always`, so a transient crash should recover by itself
   within seconds. A persistent crash loop usually means a bad image
   (recent install) or a hardware/host problem.
3. If a recent install correlates: roll back by replacing
   `current.image` with the previous artifact and
   `systemctl restart thurward`. Always keep the prior image
   alongside until you've watched the new one through a quiet hour.
4. Post-mortem: pull vsock log buffer (host-side socat output) for
   the last ~30s before crash.

### Incident: rule change deployed bad rules

1. Roll back by replacing `current.image` with the previous artifact
   and restarting. Same drop-everything-and-reconnect impact as the
   forward deploy.
2. Open a corrective PR against the firewall repo. The bad rules are
   in the diff history.
3. There is no break-glass mechanism on the running firewall — this
   is intentional ([ADR 0016](decisions/0016-image-per-change-user-deploy.md)).

### Incident: DNS resolution failing

1. Check `thurward_dns_queries_total{outcome="servfail"}` and
   `outcome="rate_limited"` rates. If `rate_limited` is high, a
   client is hammering — find them in Loki and engage.
2. Check the upstream resolver is reachable from the VM (`dig @upstream`
   from the host as a sanity check).
3. If upstream is fine but proxy is failing: pull recent SPAN events
   for `thurward.dns_proxy` — what's the error?

### Incident: drop spike on a specific rule

1. Open the dashboard's top-N FQDNs and source-IPs panels for that
   rule's events.
2. Common cause: an external service's CDN rotated; rule's FQDN list
   is out of date.
3. Fix is a rule change — open a PR against the firewall repo with the
   correction; rebuild, verify, install.

### Incident: SNAT port pool exhausted

1. `ThurwardNatPortPoolPressure` alert firing or
   `thurward_drops_total{drop_reason="nat_port_pool_exhausted"}` non-zero.
2. Find the WAN external IP from the alert labels; check
   `thurward_conntrack_entries` for that IP — a handful of LAN clients
   opening tens of thousands of connections is usually the cause.
3. Identify the offender in Loki by source IP. Engage with them /
   apply a tighter rate-limit at the rule level. If the load is
   legitimate, add a second `external_ip` to the SNAT entry and ship
   a rule change.

## Optional: fqdn_set warm replay

By default, a thurward restart loses the `fqdn_set` cache; FQDN-rule
matches cold-start until clients re-resolve. For environments where
this is intolerable, the host-side socat bridge can also record
`# METRIC thurward_fqdn_set_state {...}` snapshots emitted every 60s
to a file. On next boot, the launcher can replay the snapshot into the
new VM over vsock (a one-shot init message), populating `fqdn_set`
before opening the data path. This is optional; the v1 spec does not
require it and the reference systemd unit does not enable it.
