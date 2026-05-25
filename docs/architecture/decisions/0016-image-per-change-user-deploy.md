# ADR 0016 — Image-per-rule-change, user-driven deploy

**Status:** Accepted
**Date:** 2026-05-25
**Deciders:** magicletur
**Supersedes:** [ADR 0009](0009-gitops-rule-control-plane.md),
[ADR 0011](0011-schema-first-iac-no-custom-provider.md),
[ADR 0012](0012-reference-terraform-modules.md)

## Context

ADRs 0009, 0011, and 0012 together described a "GitOps + IaC + deploy
controller" workflow for getting rule changes from an operator's
keyboard to a running firewall. The workflow had three holes that
became impossible to ignore once chapters 06 and 07 were written
end-to-end:

1. **The "deploy controller" was never defined.** It showed up in
   ADRs 0009, 0010, and 0015 as a load-bearing trust-path component —
   a thing that pulls signed images, verifies cosign + SLSA,
   orchestrates blue/green swaps, polls heartbeats — but no ADR or
   chapter ever said what it *is*. A k8s operator? A systemd service?
   A small Go program in the repo? Reader couldn't tell; nobody
   wanted to write or maintain it.
2. **Terraform-as-IaC paid no rent.** The `terraform-thurward-rules`
   module reduced to "HCL → YAML translation + git commit on a PR
   branch". Same outcome as opening an editor on the YAML and pushing.
   Every piece of safety the GitOps path claimed (PR review, signature
   verification, reproducibility, schema validation) is available
   without Terraform, because git, cosign, SLSA, and JSON Schema work
   just fine on a hand-edited file.
3. **No reader could answer "how do I deploy thurward with my own
   rules"** from the chapter material. The IaC chapter showed a
   Terraform module committing to a "rules repo"; chapter 06 showed a
   host topology assuming the image was already there; nothing
   connected the two.

The previous-revision HA work
([ADR 0015](0015-active-passive-ha.md)) had also tied itself to the
deploy controller (controller-driven failover), so removing the
controller forces an HA decision in the same breath. That decision
(defer HA to v2) is documented in 0015's header and in
[09 — Limitations](../09-limitations.md).

## Decision

The thurward source of truth is **a single git repository** (the
firewall repo) containing both the Rust source and `rules.yaml`. Rule
changes flow:

```
operator edits rules.yaml (and/or src/)
  → (optional) signed git commit + PR + review
  → `make build` invokes `cargo build --release --target x86_64-unknown-hermit` locally
    → reproducible, pinned, byte-identical given the same versions.lock
      (ADR 0010)
    → produces a thurward unikernel image
  → (optional) `cosign sign` + SLSA-3 attestation generation
    (typically in CI; see ADR 0010)
  → operator (or their CI) publishes the image artifact wherever
    they want it (GitHub release, OCI registry, internal bucket, file
    copied to a USB stick)
  → operator (or their config-management) runs the installer on the
    target host:
      - `cosign verify` + `slsa-verifier` against the artifact (ADR 0010)
      - swap the running image (per 06-deployment.md, per launcher)
```

No process on the firewall ever accepts a rule change at runtime. No
custom controller. No separate "rules repo". No Terraform.

### What replaces each removed piece

| Removed                                         | Replaced by                                                              |
| ----------------------------------------------- | ------------------------------------------------------------------------ |
| Separate rules repo                             | `rules.yaml` in the firewall repo                                        |
| `terraform-thurward-rules` module               | Editing `rules.yaml` in an editor with the JSON Schema attached          |
| `terraform-thurward-infra` module               | The launcher commands in [06 — Deployment](../06-deployment.md)          |
| Deploy controller pulling images                | Operator runs the install step (script, Ansible task, manual `cp`)       |
| Deploy controller verifying cosign + SLSA       | `cosign verify` + `slsa-verifier` invoked in the install step (ADR 0010) |
| Deploy controller orchestrating blue/green swap | Stop the running unit, start the new image (per launcher)                |
| Controller-driven HA failover                   | HA is deferred to v2 — see ADR 0015 superseded header                    |

### Why this is *not* a downgrade

The previous architecture's GitOps claims were all really claims about
git + cosign + SLSA + JSON Schema. None of those require a controller.
A user with `git`, `cosign`, `slsa-verifier`, the Cargo + Hermit
toolchain ([ADR 0017](0017-hermit-rust-substrate.md)), and the
firewall repo has every property the previous architecture promised
*except* automated cross-host rollouts — which were always a fleet
feature that v1 doesn't target.

What is genuinely gone:

- **Fleet automation.** v1 expects a human (or that human's
  config-management of choice) to run the install on the target host.
  There is no "kubectl apply this rule change to every firewall in the
  fleet" path.
- **Sub-second HA failover.** Deferred to v2 along with ADR 0015.

These omissions are explicit and listed in
[09 — Limitations](../09-limitations.md).

### The schema is still the contract

`schemas/rules.schema.json` (originally introduced by
[ADR 0011](0011-schema-first-iac-no-custom-provider.md)) remains the
authoritative description of a valid `rules.yaml`. The build-time
compiler validates against it; editors with JSON-Schema awareness use
it for autocomplete and inline errors. This part of ADR 0011 was sound
and survives unchanged here.

## Consequences

- (+) A reader can answer "how do I deploy with my own rules" from
  README + 00-overview + 06-deployment alone. No intermediate
  components to define.
- (+) Zero custom infrastructure code to maintain. `cargo` + Hermit,
  `git`, `cosign`, `slsa-verifier`, and the user's launcher of choice
  are all off-the-shelf.
- (+) The firewall still has no inbound management surface — the
  threat-model conclusion ADR 0009 reached survives intact; only the
  ceremony around it changes.
- (−) No fleet rollout story. Operators with multiple firewalls do
  the rollout themselves via whatever they already use (Ansible,
  Puppet, hand-managed). Could become a problem worth solving in v2;
  not v1.
- (−) Verification is user-initiated. A user who skips
  `cosign verify` is running an unverified image. The previous
  framing forced verification at the controller; the new framing
  trusts the user to do it. The recommended install scripts in
  [06 — Deployment](../06-deployment.md) bake the verification in,
  making it the easy path.
- (○) The git workflow (signed commits, PR review, branch protection)
  is unchanged but now applies to the *firewall* repo rather than a
  separate rules repo. Same controls; one fewer repo to manage.

## Alternatives considered

- **Keep the GitOps + deploy controller story; finally define and ship
  the controller.** Would close the "what is this thing" hole at the
  cost of writing and maintaining a non-trivial new component
  (image puller, signature verifier, swap orchestrator, health
  poller). The whole point of the previous design was to avoid runtime
  surface on the firewall; adding a host-side daemon to *replace*
  that runtime surface trades the same complexity around. Rejected:
  user explicitly asked to drop the GitOps aspect.
- **Keep Terraform as one of several IaC options.** Mixed signal
  about whether IaC is the recommended path. The schema-as-API
  framing handles every IaC use case without privileging Terraform.
  Rejected.
- **Build composition primitives now** (rules.d/ fragments merged in
  CI). Real value if multiple teams own subsets of rules, but
  premature for v1 single-deployment scope. Documented as a
  potential v2 addition, not implemented.

## Relation to other ADRs

- [ADR 0005](0005-build-time-rule-compilation.md) — unchanged. Rules
  still compile into the image; this ADR just changes *who* triggers
  the compile and how the image gets to the host.
- [ADR 0009](0009-gitops-rule-control-plane.md) — superseded by this
  ADR. Threat-comparison table preserved in 0009 for history.
- [ADR 0010](0010-supply-chain-hardening.md) — revised in this same
  pass to reframe enforcement as user-verifiable rather than
  controller-enforced.
- [ADR 0011](0011-schema-first-iac-no-custom-provider.md) — superseded
  by this ADR. The schema-as-API thread continues here; the IaC
  framing does not.
- [ADR 0012](0012-reference-terraform-modules.md) — superseded by
  this ADR.
- [ADR 0015](0015-active-passive-ha.md) — deferred to v2 in the same
  pass; documented as a known v1 limitation.
