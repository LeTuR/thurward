# ADR 0009 — GitOps as the rule control plane (superseded)

**Status:** Superseded by [ADR 0016](0016-image-per-change-user-deploy.md)
**Date:** 2026-05-23
**Superseded:** 2026-05-25
**Deciders:** magicletur

> **Superseded.** thurward does not ship a GitOps pipeline or a
> deploy controller. The user-driven build-and-install workflow in
> [ADR 0016](0016-image-per-change-user-deploy.md) replaces this
> ADR's mechanism. The *conclusion* this ADR reached — **the firewall
> exposes no inbound management surface** — is unchanged and now
> lives in ADR 0016; the threat-comparison table below is preserved
> because that's the canonical justification for that conclusion.

## What survives: the threat comparison

The load-bearing question this ADR answered: should the firewall
accept rule changes at runtime via an admin API, or should rules be
compiled into the image and applied via image swap?

| Threat                                  | Build-time (chosen)                                 | Runtime admin API (rejected)                                    |
| --------------------------------------- | --------------------------------------------------- | --------------------------------------------------------------- |
| Network attacker reaches firewall       | No inbound mgmt port. Zero new network surface.    | mTLS endpoint exposed; auth-bypass CVEs are reachable.          |
| Compromised CI / build environment      | Can ship a malicious image (high impact)            | Same — CI still builds the firewall                             |
| Compromised developer laptop            | Bad commit → caught by PR review                    | Bad call → instant production effect                            |
| Compromised admin credentials           | Need git push **and** build access (multi-step)     | Single-step compromise                                          |
| Replay / downgrade on rule changes      | N/A — no protocol                                   | Real risk; needs protocol-level mitigation                      |
| Recovery from a bad rule change         | `git revert` + rebuild — fully auditable            | Push correction — auditable only if every call is logged well   |
| Insider exfiltration via "test" rule    | Visible in PR diff; reviewable                      | May not surface in any code review                              |

The CI/build-environment row is the only line where build-time looks
worse, and it has a documented mitigation set
([ADR 0010](0010-supply-chain-hardening.md)). The runtime-API risks
have no analogous mitigation: once an admin API exists, every CVE in
its code is reachable from somewhere on the network, and every
credential is a single-step compromise vector.

**This table is the canonical reference** for the
no-inbound-management stance. ADRs 0005, 0010, and 0016 all point to
it.

## What did not survive

- The "deploy controller pulls signed images, verifies, and runs
  blue/green swap" mechanism. ADR 0016 retired it because nobody
  defined or wanted to maintain that component; the verification
  step (`cosign verify` + `slsa-verifier`) is now run by the
  operator (or their config-management) at install time.
- The "separate rules repo" workflow. Under ADR 0016, `rules.yaml`
  lives alongside the firewall source in a single git repo.
- The IaC-vendor pipeline framing
  ([ADR 0011](0011-schema-first-iac-no-custom-provider.md),
  [ADR 0012](0012-reference-terraform-modules.md)) is also gone;
  editing YAML directly is the documented workflow.

## What replaced this ADR

[ADR 0016](0016-image-per-change-user-deploy.md) lays out the
current shape: single firewall repo with `rules.yaml`, local
`make build` (wrapping `cargo build --target x86_64-unknown-hermit`),
optional `cosign verify` + `slsa-verifier` before install, no
controller, no automated fleet rollout.
