# ADR 0005 — Build-time rule compilation

**Status:** Accepted
**Date:** 2026-05-23
**Deciders:** magicletur

## Context

Firewall rules need to get from a human-edited source (YAML) into the
running filter engine. Conventional firewalls (nftables, iptables, pf)
read rules at runtime — the kernel exposes a syscall or netlink interface
and a userspace tool pushes the rules in. This works because the kernel
is mutable, the rules are mutable, and there's an admin process to do
the pushing.

A unikernel has none of those affordances. There's no host shell, no
admin daemon, no syscall surface from outside the VM. There are two ways
to deal with rules in this environment: build them into the image at
compile time, or expose a runtime control channel (mgmt NIC, vsock RPC).
The control-channel choice is treated in
[ADR 0016](0016-image-per-change-user-deploy.md) (and historically in
the now-superseded [ADR 0009](0009-gitops-rule-control-plane.md), which
preserves the threat-comparison table); this ADR is about *how* rules
become bytes inside the image.

## Decision

Rules live in `rules.yaml` at the root of the source repo. A Cargo
`build.rs` script (under [ADR 0017](0017-hermit-rust-substrate.md))
parses `rules.yaml`, validates it against `schemas/rules.schema.json`,
and emits `$OUT_DIR/rules_table.rs` — a packed Rust module of
`pub static` arrays that the firewall crate `include!`s. The filter
engine matches against that table. See
[03 — Rule model](../03-rule-model.md) for the generated-code shape.

`rules.yaml` is the only operator-facing artifact for rules. The
generated Rust module is regenerated every build and never committed
(it lives in Cargo's `$OUT_DIR`, outside the repo entirely).

## Consequences

- (+) Zero runtime configuration surface — no parser, no admin port,
  no auth, no race conditions. The rule table is read-only `const`
  data in the image.
- (+) Compile-time validation catches malformed rules before they ship:
  unknown rule types, CIDR parse errors, port range overflows all fail
  the build.
- (+) Per-rule counters can be backed by static arrays sized at compile
  time — no allocation in the hot path.
- (+) Pairs naturally with [ADR 0016](0016-image-per-change-user-deploy.md):
  every rule change is a code change is a build is a deploy.
- (−) Every rule change requires a rebuild and redeploy. With
  Firecracker boot times (~10 ms) and a launcher restart this is seconds
  end-to-end, but it's still seconds, not milliseconds; on bare-metal
  it's a reflash + reboot.
- (−) Operators cannot "twiddle one rule" — they have to ship a new
  artifact. This is a *feature* under [ADR 0016](0016-image-per-change-user-deploy.md)
  but a debt under operator convenience.
- (○) The `fqdn_set` cache (see [ADR 0004](0004-dns-proxy-for-fqdn-rules.md))
  is necessarily mutable at runtime because DNS responses change. It is
  the *only* mutable rule-related state in the image.

## Alternatives considered

- **Runtime config file mounted via virtio-fs.** Would let rules change
  without rebuild. Rejected because it reintroduces a runtime parser
  (attack surface), a file-watch loop (complexity), and a "rules
  changed but the running flows used the old rules" race that's not
  obviously safe.
- **Admin RPC over a dedicated mgmt NIC or vsock.** Most flexible.
  Rejected; see [ADR 0009](0009-gitops-rule-control-plane.md) (now
  superseded but its threat-comparison table is the canonical
  reference) for the threat-model analysis.
- **Embed rules as a compressed blob, parsed at boot.** Trades
  build-time validation for a slightly faster rebuild cycle when only
  rules change. Rejected — the savings don't justify the runtime
  parser and the loss of compile-time type checking.
