---
name: Bug report
about: Behaviour that contradicts the architecture docs or `tests/scenarios.md`
title: "bug: "
labels: ["bug"]
assignees: []
---

<!-- Don't use this template for security issues. See SECURITY.md. -->

## Summary

<!-- One sentence: what behaviour did you observe vs. what was expected? -->

## Affected scenario / chapter

<!-- Link to the `tests/scenarios.md` S-NN entry, ADR, or architecture chapter
that this bug contradicts. If the docs and the bug disagree, the docs are
the source of truth — call out the disagreement explicitly. -->

## Reproduction

- thurward version / image tag (or commit hash):
- Host / hypervisor (QEMU, Firecracker, KVM version):
- Minimal `rules.yaml`:

```yaml
# minimal rule set that triggers the bug
```

- Packet trace / pcap (link or attached):
- Expected verdict:
- Actual verdict:

## Logs / observability

<!-- ECS JSON events emitted on vsock around the time of the bug, if any. -->

## Anything else

<!-- Workarounds you've found, related issues, etc. -->
