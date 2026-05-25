# ADR 0012 — Reference Terraform modules (superseded)

**Status:** Superseded by [ADR 0016](0016-image-per-change-user-deploy.md)
**Date:** 2026-05-23
**Superseded:** 2026-05-25
**Deciders:** magicletur

> **Superseded.** The two reference Terraform modules
> (`terraform-thurward-rules` and `terraform-thurward-infra`) that
> this ADR described are retired. Neither is shipped, documented, or
> endorsed.
>
> - `terraform-thurward-rules` reduced to "HCL → YAML translation +
>   git commit on a PR branch", which is roundabout for what is
>   fundamentally an editor task. ADR 0016 replaces it with
>   "edit `rules.yaml`, rebuild the image".
> - `terraform-thurward-infra` described host bridges and a deploy
>   controller that ADR 0016 has now removed. The remaining host
>   plumbing (the two bridges, the systemd unit for Path B) is
>   shown directly in [06 — Deployment](../06-deployment.md) as
>   shell commands; a few `ip link add` + `systemctl enable` lines
>   don't need a Terraform module.
>
> If a community contributor wants a Terraform-shaped interface
> against `schemas/rules.schema.json`, they're welcome to build one
> — the schema is the contract.
