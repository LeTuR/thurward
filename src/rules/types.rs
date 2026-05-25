//! Runtime types for the compiled rule table.
//!
//! These types are shared between `build.rs` (which constructs the `Rule`
//! literals) and the dataplane (which scans `RULES` per packet). They're
//! deliberately small, `Copy`, and `const`-constructable so the generated
//! `RULES` slice lives in `.rodata` with zero runtime allocation — see
//! ADR 0005 / `docs/architecture/03-rule-model.md`.
//!
//! Phase-1 scope: `state` and `destination_fqdn` are captured but not yet
//! matched by the filter (conntrack and the DNS proxy land in later PRs).

/// What a rule does when it matches.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Accept,
    Deny,
}

/// LAN→WAN (egress), WAN→LAN (ingress), or unconstrained.
///
/// A rule whose YAML omits `direction` becomes `Either`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    Egress,
    Ingress,
    Either,
}

/// L4 protocol filter. `Any` matches packets of any supported protocol.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Any,
}

/// Conntrack state predicate. v1 supports `Established`; phase-2 PR adds
/// the runtime matching. Until then this is recorded verbatim and ignored
/// by the filter.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StateSpec {
    Any,
    Established,
}

/// A destination-port predicate.
///
/// `Any` is the no-port-constraint case (the rule matches at L3 only).
/// `Single` is the exact-port case. `Range(lo, hi)` is the inclusive range.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PortSpec {
    Any,
    Single(u16),
    Range(u16, u16),
}

/// An IPv4 CIDR. `addr` is the network base in big-endian-numerical form
/// (i.e. `127.0.0.1` ↔ `0x7F000001`); `prefix_len` is in bits, `0..=32`.
///
/// `prefix_len = 32` is the host route case (`/32`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CidrV4 {
    pub addr: u32,
    pub prefix_len: u8,
}

impl CidrV4 {
    /// Does `ip` (also in numerical form) fall inside this CIDR?
    pub const fn contains(&self, ip: u32) -> bool {
        if self.prefix_len == 0 {
            return true;
        }
        let mask: u32 = !0u32 << (32 - self.prefix_len);
        (ip & mask) == (self.addr & mask)
    }
}

/// One compiled rule. All fields are `Option`-flavoured so a missing YAML
/// field becomes "unconstrained" rather than "matches nothing".
///
/// Field order chosen for readable generated output, not memory layout —
/// the `Rule` is too small to bother packing.
#[derive(Copy, Clone, Debug)]
pub struct Rule {
    /// Stable identifier from `rules.yaml`. Used in logs (ECS `rule.id`).
    pub id: &'static str,
    /// Human-readable name. Used in logs (ECS `rule.name`). Empty if the
    /// YAML omitted `name`.
    pub name: &'static str,
    pub action: Action,
    pub direction: Direction,
    pub source: Option<CidrV4>,
    pub destination: Option<CidrV4>,
    pub destination_port: PortSpec,
    /// List of FQDN patterns (exact or `*.suffix.example`). Phase-1 records
    /// this but does NOT match against it — the DNS proxy lands in a later
    /// PR (ADR 0004). A non-empty list means "this rule's predicate
    /// requires runtime FQDN resolution and the filter must currently
    /// treat it as no-match."
    pub destination_fqdn: &'static [&'static str],
    pub protocol: Protocol,
    pub state: StateSpec,
}

impl Rule {
    /// Does this rule need a runtime that doesn't exist yet (FQDN
    /// resolution or conntrack state)? Phase-1 callers can use this to
    /// skip such rules rather than match them incorrectly.
    pub const fn needs_future_runtime(&self) -> bool {
        !self.destination_fqdn.is_empty() || matches!(self.state, StateSpec::Established)
    }
}
