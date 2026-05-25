# ADR 0015 — Active/passive HA with vsock state sync (deferred to v2)

**Status:** Deferred to v2 (drafted 2026-05-24, reverted from v1 2026-05-25)
**Date:** 2026-05-24
**Deciders:** magicletur
**Related:** [ADR 0013](0013-zig-fast-path-on-uknetdev.md) (provides
the conntrack table that would be replicated),
[ADR 0014](0014-stateful-nat.md) (is why HA matters at all —
NAT-makes-crashes-painful is the motivation).

> **Deferred to v2.** v1 thurward is single-VM. This ADR is preserved
> as a starting point for the v2 author, but parts of the original
> draft no longer apply because the "deploy controller" it leaned on
> for the failover trigger was retired by
> [ADR 0016](0016-image-per-change-user-deploy.md). Sections below
> are marked **REUSABLE** (carry forward to v2) or **REPLACE** (the
> original mechanism doesn't fit any more; v2 has to pick a new one).

## Why this ADR exists at all

NAT in v1 ([ADR 0014](0014-stateful-nat.md)) means a thurward crash
drops every NAT binding, so *every* in-flight flow breaks on recovery
— not just half-open ones. The original v1 deployment story (one VM,
fail-closed on crash, brief gap during image swap) was tolerable
under a pure allow/deny middlebox but is much worse once NAT is in
the picture.

That cost is **acknowledged and accepted** in v1
([09 — Limitations](../09-limitations.md)) for the sake of shipping
something coherent. v2 needs to come back to it.

## REUSABLE — Pair topology

The shape of an HA deployment carries forward to v2 unchanged:

- Two unikernel VMs on the same hypervisor host (or paired hosts —
  the design works either way): `active` and `standby`.
- Both attached to the same LAN and WAN L2 segments
  (`br-lan` / `br-wan` from [06 — Deployment](../06-deployment.md)).
- Both run the same image SHA — pairs cannot be heterogeneous.
- Only the active VM owns the floating LAN-side IP and the WAN-side
  IP. The standby is silent on the data NICs until promoted.

## REUSABLE — State replication channel

The vsock-based state replication design is the most reusable piece
of this ADR:

- A **second vsock port** (default 9001), separate from the
  observability stream on 9000
  ([ADR 0006](0006-vsock-for-observability-egress.md)).
- The active VM streams **conntrack and NAT-binding deltas** as
  compact binary records: `{create, update, delete}` × tuple. The
  host forwards the stream to the standby's symmetric port; if the
  pair is split across hosts, a small host-side relay does it.
- Replication is **best-effort, eventually consistent**. The active
  side does not block on acknowledgement; the standby applies what
  it receives. On failover, recent (sub-second) updates may be lost.

This part is implementation-language-independent (Rust per
[ADR 0017](0017-hermit-rust-substrate.md) is fine) and doesn't depend
on which side triggers failover.

## REUSABLE — What survives failover

| Flow type                          | Survives?                                   |
| ---------------------------------- | ------------------------------------------- |
| Established TCP (replicated)       | Yes (modulo sub-second window of loss)      |
| Established UDP / ICMP echo        | Yes (modulo sub-second window)              |
| Half-open TCP (SYN sent, no ACK)   | No — replication races the handshake        |
| DNS proxy in-flight queries        | No — re-resolved by clients on timeout      |
| `fqdn_set` cache                   | Replicated continuously; survives           |
| Per-rule counters                  | Not replicated; reset on standby            |

## REPLACE — Failover trigger

The original draft chose **controller-driven failover**: a deploy
controller polled `thurward_ha_heartbeat_seconds_ago` over vsock at
100 ms intervals and issued a gratuitous ARP from the standby on
detected loss. That mechanism **no longer applies** —
[ADR 0016](0016-image-per-change-user-deploy.md) retired the deploy
controller entirely. v2 must pick a different failover trigger.

Candidate replacements for v2 to evaluate:

- **Host-side `keepalived` (VRRP).** Standard, well-understood. The
  unikernel doesn't run VRRP itself; the host-side keepalived
  watches the active VM's vsock health metric (or a TAP-device
  liveness signal) and migrates the floating IP. Trade-off: adds a
  host-side daemon dependency.
- **Small systemd watchdog on the host.** A few-hundred-line
  service that watches the vsock health stream and runs the
  promotion script on failure. Cheaper than keepalived; same shape.
- **Active-active with flow hashing.** Both VMs receive traffic via
  L2 ECMP or a smart switch; failure of one halves capacity but
  doesn't cause full outage. Pairs naturally with the multi-queue
  stretch in [ADR 0013](0013-zig-fast-path-on-uknetdev.md). More
  complex; needs symmetric routing or shared conntrack lookup.
- **In-VM VRRP-like heartbeat.** Originally rejected because it
  adds an inbound protocol surface to the firewall; v2 should
  weigh that against the simplicity of not depending on host-side
  infrastructure.

The original draft's rejection of in-VM heartbeat (over inbound
trust-surface concerns) is still a reasonable position but worth
revisiting in v2 against the maintenance cost of host-side options.

## REUSABLE — Out of scope (carries forward to v2)

- **Cross-region / cross-site HA.** The same-L2 assumption for
  floating-IP migration limits this design to a single L2 segment.
  Multi-site is a different problem class.
- **Synchronous replication with quorum.** Would gate the data path
  on the standby's ACK; defeats the perf target from ADR 0013.
- **Split-brain prevention via fencing.** Single floating IP makes
  split-brain unreachable in the documented topology; richer
  fencing only matters under in-VM heartbeat candidates.

## Consequences if v2 ships HA along these lines

- (+) Sub-second failover closes the "VM crash = total outage" hole
  introduced by NAT.
- (+) Reuses existing primitives (bridges, vsock transport).
- (−) Two VMs per deployment doubles the host resource footprint.
- (−) State replication is best-effort: some flows still break on
  failover; the SLO is "sub-second outage", not "zero loss".
- (−) Whichever failover trigger v2 picks introduces a new
  dependency (host daemon, switch, etc.).

## Relation to other ADRs

- [ADR 0006](0006-vsock-for-observability-egress.md) — vsock
  channel re-used (different port) for state replication.
- [ADR 0013](0013-zig-fast-path-on-uknetdev.md) — provides the
  conntrack table that this design replicates.
- [ADR 0014](0014-stateful-nat.md) — provides the NAT bindings
  that this design replicates; is also the reason HA stopped being
  deferrable for v2.
- [ADR 0016](0016-image-per-change-user-deploy.md) — retired the
  deploy controller that the original draft's failover trigger
  depended on.
- [ADR 0017](0017-hermit-rust-substrate.md) — substrate; the state
  sync design is implementation-language-independent and carries
  over to the Rust crate naturally.
