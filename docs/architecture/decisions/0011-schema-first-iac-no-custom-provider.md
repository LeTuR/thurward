# ADR 0011 — Schema-first IaC, no custom Terraform provider (superseded)

**Status:** Superseded by [ADR 0016](0016-image-per-change-user-deploy.md)
**Date:** 2026-05-23
**Superseded:** 2026-05-25
**Deciders:** magicletur

> **Superseded.** thurward does not ship an "IaC" surface at all.
> Rules live in `rules.yaml` in the firewall repo; users edit the
> YAML and rebuild the image. See
> [ADR 0016](0016-image-per-change-user-deploy.md).

## What survives

The piece that mattered — and still matters — is the JSON Schema
itself: `schemas/rules.schema.json`. It remains:

- The authoritative description of a valid `rules.yaml`.
- The contract that the build-time rule compiler
  ([ADR 0005](0005-build-time-rule-compilation.md)) validates against.
- The thing that editors with JSON-Schema awareness (VS Code,
  Helix) read for autocomplete and inline error reporting while
  editing `rules.yaml`.

ADR 0016 preserves the schema in that role.

## What did not survive

The "IaC pipeline" framing — rendering YAML from Terraform / Pulumi /
Ansible / Helm — added no declarative value over editing the YAML
directly, because the YAML is already declarative and the schema
already validates it. ADR 0016 retired the IaC framing entirely.

Anyone who still wants a Terraform-shaped interface to thurward can
write their own provider against `schemas/rules.schema.json` — the
project doesn't document or endorse one.
