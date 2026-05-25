# Security policy

thurward is a network firewall. Vulnerabilities in firewalls compromise
the boundary they're protecting, so we treat all reports as serious by
default.

## Supported versions

thurward is **pre-v1** (architecture-docs-first). No images are published
yet, so there is nothing in production to receive a security fix. Once
v1 ships, only the latest released image will receive fixes; older
images will be marked end-of-life. See ADR-0016 (image-per-change
deploy) — every change is a rebuild and redeploy, not a patch.

| Version    | Status      | Receives fixes |
| ---------- | ----------- | -------------- |
| pre-v1     | development | n/a            |
| v1 (future)| latest only | yes            |
| v1 (older) | EOL on next release | no    |

## Reporting a vulnerability

**Please do not open a public GitHub issue.** Use GitHub's private
vulnerability reporting at
<https://github.com/LeTuR/thurward/security/advisories/new>.

If private reporting is unavailable, email the maintainer listed on
[GitHub](https://github.com/LeTuR). Encrypt sensitive details if you
can; expect an acknowledgement within seven days.

Include where possible:

- the architecture chapter, ADR, or rule construct involved
- a minimal `rules.yaml` and packet trace that reproduces the issue
- the image tag (when there's an image) or the commit hash
- impact assessment in your own words

## What we treat as in scope

- L3/L4 filter or NAT decisions that violate `tests/scenarios.md`
- DNS-proxy behaviour that lets a client bypass FQDN policy without
  using one of the documented bypasses (DoH/DoT/hardcoded `8.8.8.8` —
  see `docs/architecture/09-limitations.md`)
- Build-time rule compilation that admits an invalid `rules.yaml`
- Image-signing or SLSA-provenance bypasses (see ADR-0010)
- Anything that lets an attacker turn off `default-deny` without a new
  signed image

## What is out of scope

Documented limitations in
[`docs/architecture/09-limitations.md`](docs/architecture/09-limitations.md)
are intentional and not vulnerabilities:

- IPv6 dropped unconditionally (v1)
- No hairpin NAT, no ALGs, no CGNAT-scale port allocation
- No HA / failover in v1
- No L7 inspection
- DoH/DoT/hardcoded-resolver DNS bypass (operational mitigation only)
- No hot rule reload (every change is a new signed image)

Reports against these will be closed with a pointer to the limitations
chapter.

## Supply-chain security

thurward follows the controls in
[ADR-0010 — supply-chain hardening](docs/architecture/decisions/0010-supply-chain-hardening.md):

- reproducible builds with `versions.lock`
- SLSA-v1.0 provenance attestation on every image
- cosign signatures verified by the launcher at boot
  (`tests/scenarios.md` S-50)
- provenance traceability from running image to `rules.yaml` commit
  (`tests/scenarios.md` S-51)

Reports about the supply chain itself (build infrastructure, signing
keys, attestation contents) are explicitly in scope.
