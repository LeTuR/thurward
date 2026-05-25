# ADR 0010 — Supply-chain hardening

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

Build-time rule compilation ([ADR 0005](0005-build-time-rule-compilation.md))
and user-driven install ([ADR 0016](0016-image-per-change-user-deploy.md))
together mean a thurward image is built somewhere (CI, a developer
workstation), shipped somewhere (a registry, a release page, a USB
stick), and run somewhere else. Every hop in that path is an attack
surface — and the only thing that closes the gap at install time is
the operator running `cosign verify` on the artifact in their hand.

This ADR's job is to make that verification actually mean something:
to pin the inputs, make the build reproducible, sign and attest the
output, and document the verification commands so the operator can
trust what they're about to install.

The threats this ADR addresses:

- Compromised CI runner injecting malicious code into the image.
- Compromised upstream dependency (Unikraft + selected `lib-*`
  components, Rust toolchain, smoltcp, or any other transitive crate
  in `Cargo.lock`). See
  [ADR 0018](0018-substrate-pivot-unikraft.md) for the substrate
  stack and [ADR 0017](0017-hermit-rust-substrate.md) for the
  language and parser-library choices.
- Compromised developer pushing bad rules / bad code without review.
- Compromised image registry (or any intermediate mirror) serving a
  swapped image to a user.
- A user installing an image they didn't actually build, without
  noticing.

These threats are well-studied. SLSA, Sigstore, and reproducible-build
practice exist to address them. We adopt them deliberately rather than
inventing our own.

## Decision

The build pipeline implements these practices end-to-end:

1. **Pinned dependencies.** Under
   [ADR 0017](0017-hermit-rust-substrate.md) (language + parser) and
   [ADR 0018](0018-substrate-pivot-unikraft.md) (substrate):
   `rust-toolchain.toml` (rustup channel + components), `Cargo.lock`
   (every crate + transitive), the Unikraft revision plus each
   selected `lib-*` component revision, and the smoltcp crate version
   are all pinned in a single `versions.lock` file (it references the
   individual artifacts above and acts as the single source of truth
   a build either matches or fails). CI fails if any drift.
2. **Reproducible builds.** Given the same source tree, the same
   `versions.lock`, and the same build environment image, two builds
   produce byte-identical output. The build environment itself is
   pinned (Nix flake or pinned container image SHA).
3. **Hermetic build environment.** CI builds run in a network-isolated
   container; dependencies are vendored or fetched against pinned
   SHAs from a content-addressed proxy.
4. **Signed commits.** Commits to the firewall repo (which now contains
   both source and `rules.yaml` per
   [ADR 0016](0016-image-per-change-user-deploy.md)) require Sigstore
   commit signatures (or SSH-signed commits, accepted alternative).
   Branch protection rejects unsigned commits.
5. **Multi-party PR approval.** Branch protection requires N reviewers
   (recommend N≥2) on the firewall repo; main branch is protected.
6. **Cosign-signed artifacts with SLSA-3 provenance.** The final
   unikernel image is signed with `cosign sign` and accompanies a
   SLSA-3 provenance attestation linking image-SHA → git-SHA → CI run.
7. **User-verifiable admission.** Each image ships its cosign signature
   and SLSA-3 attestation alongside the artifact. Before installing,
   the user runs `cosign verify` + `slsa-verifier verify-artifact`
   (commands shown in [06 — Deployment](../06-deployment.md)) to
   confirm: (a) the signature matches a configured trusted key,
   (b) the SLSA provenance points to the expected source repo, (c)
   the source SHA was on the protected branch when built. Failures
   mean the user does not install. Operators with stricter needs
   (regulated environments) plug into their own admission mechanism
   — e.g. an in-house Kyverno/Conftest gate or a systemd
   `ExecStartPre` running the same verifiers — and thurward does
   not ship one in the box. The verification commands are the
   contract; *who* runs them is a deployment-policy choice.
8. **No separate rules repo.** Since [ADR 0016](0016-image-per-change-user-deploy.md),
   `rules.yaml` lives alongside the firewall source. The
   "rules-repo / code-repo split" the previous version of this
   ADR recommended is gone; the privilege model is now the
   normal git permissions on the firewall repo.

## Consequences

- (+) Closes the CI-compromise gap that existed under the previous
  GitOps framing and persists under the user-driven framing of
  [ADR 0016](0016-image-per-change-user-deploy.md).
- (+) Reproducible builds let any third party verify the binary they're
  about to run matches the source — no trust in the build farm needed.
- (+) User-verifiable admission means a stolen image registry can't
  silently ship an unsigned-or-misattested image to anyone running the
  documented `cosign verify` step.
- (+) The verification commands and trust roots are documented; users
  who want stricter admission (an enterprise gate, a Kyverno policy,
  a systemd ExecStartPre check) wire the same verifiers into their
  own controls.
- (−) CI complexity goes up significantly. Reproducible builds are
  notoriously fiddly to maintain — every new dep, every toolchain
  upgrade, every embedded timestamp risks breaking reproducibility.
- (−) Sigstore / cosign / SLSA tooling adds operational weight: keys
  to rotate, attestation storage, signature verification at deploy
  time.
- (−) Multi-party approval slows down routine rule changes. Mitigation:
  emergency-break-glass procedure documented in [07 — Operations](../07-operations.md).
- (○) These practices are not unique to thurward; the same principles
  apply to anything serious shipping containers or unikernels.

## Alternatives considered

- **Trust the CI farm.** Standard for hobby projects; unacceptable
  for a firewall.
- **Implement only some practices.** E.g. signed artifacts without
  reproducible builds. Rejected — each practice closes a different
  gap; partial adoption leaves obvious holes.
- **Use a fully managed build platform** (a hosted CI / SaaS build
  service offering signed-and-attested builds out of the box).
  Reduces operational burden but shifts trust to a third party. Not
  rejected outright — the docs describe what the pipeline must
  achieve; *who* operates it is a later choice.
