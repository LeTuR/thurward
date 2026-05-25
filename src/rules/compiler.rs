//! Compile `rules.yaml` to Rust source code embedded in the binary.
//!
//! Used by `build.rs` to emit `$OUT_DIR/rules_table.rs`. Per ADR 0005 /
//! `docs/architecture/03-rule-model.md`, the generated module exports:
//!
//! ```text
//! pub const DEFAULT_ACTION: Action;
//! pub static RULES: &[Rule] = &[ /* one entry per rules.yaml rule */ ];
//! ```
//!
//! Phase-1 scope: only the `defaults.default_action` and `rules` sections
//! drive runtime behaviour. `defaults.*` other than `default_action` and
//! the entire `nat` section are parsed but emitted as `//` comments so
//! they're visible in the generated file without affecting runtime. The
//! conntrack-timeouts and SNAT/DNAT runtime arrives in later PRs.

use serde::Deserialize;
use std::fmt::Write;
use std::net::Ipv4Addr;

/// Errors the compiler can produce. Stringified back in `build.rs` so
/// `cargo build` failures point to the offending YAML line.
#[derive(Debug)]
pub enum CompileError {
    Yaml(serde_yml::Error),
    BadCidr(String),
    BadPort(String),
    BadProtocol(String),
    BadAction(String),
    BadDirection(String),
    BadState(String),
    MissingId,
    /// `destination` and `destination_fqdn` are mutually exclusive per
    /// the schema. The compiler rejects rules that set both.
    DestinationConflict {
        rule_id: String,
    },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yaml(e) => write!(f, "rules.yaml is not valid YAML: {e}"),
            Self::BadCidr(s) => write!(f, "not a valid IPv4 CIDR: {s:?}"),
            Self::BadPort(s) => write!(f, "not a valid port spec: {s:?}"),
            Self::BadProtocol(s) => write!(f, "not a valid protocol: {s:?}"),
            Self::BadAction(s) => write!(f, "not a valid action: {s:?}"),
            Self::BadDirection(s) => write!(f, "not a valid direction: {s:?}"),
            Self::BadState(s) => write!(f, "not a valid state: {s:?}"),
            Self::MissingId => write!(f, "every rule needs an `id`"),
            Self::DestinationConflict { rule_id } => write!(
                f,
                "rule {rule_id:?}: `destination` and `destination_fqdn` are mutually exclusive"
            ),
        }
    }
}

impl std::error::Error for CompileError {}

// --- The YAML shape ----------------------------------------------------------
//
// These structs mirror `schemas/rules.schema.json`. They're build-side only
// (use String/Vec freely); the `Rule` literals we emit reference the
// `&'static`-typed runtime types in `src/rules/types.rs`.

#[derive(Deserialize, Debug)]
pub struct RulesYaml {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub nat: NatBlock,
    pub rules: Vec<RuleYaml>,
}

#[derive(Deserialize, Debug)]
pub struct Defaults {
    #[serde(default = "default_action_deny")]
    pub default_action: String,
    #[serde(default)]
    pub upstream_dns: Option<String>,
    #[serde(default)]
    pub flow_trace_sample_accept: Option<f64>,
    #[serde(default)]
    pub flow_trace_sample_drop: Option<f64>,
    #[serde(default)]
    pub dns_qps_per_client: Option<u32>,
    #[serde(default)]
    pub conntrack_timeouts: Option<serde_yml::Value>,
}

// `#[derive(Default)]` would give us `default_action = ""`, which then trips
// `parse_action`. We need the absent-`defaults`-block case to mean "deny" —
// the schema's stated default — so we hand-roll `Default` accordingly.
impl Default for Defaults {
    fn default() -> Self {
        Self {
            default_action: default_action_deny(),
            upstream_dns: None,
            flow_trace_sample_accept: None,
            flow_trace_sample_drop: None,
            dns_qps_per_client: None,
            conntrack_timeouts: None,
        }
    }
}

fn default_action_deny() -> String {
    "deny".to_string()
}

#[derive(Deserialize, Debug, Default)]
pub struct NatBlock {
    #[serde(default)]
    pub snat: Vec<serde_yml::Value>,
    #[serde(default)]
    pub dnat: Vec<serde_yml::Value>,
}

#[derive(Deserialize, Debug)]
pub struct RuleYaml {
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub action: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub destination_port: Option<PortValue>,
    #[serde(default)]
    pub destination_fqdn: Option<Vec<String>>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

/// `destination_port` can be either a bare integer or a `"lo-hi"` string —
/// the schema declares this union. Untagged enum lets serde pick the right
/// variant.
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum PortValue {
    Single(u16),
    Range(String),
}

// --- Public entry point ------------------------------------------------------

/// Parse `yaml` and return the Rust source for the generated module.
///
/// On success, the returned string can be written verbatim to a `.rs`
/// file and `include!`d from `src/lib.rs`.
pub fn compile(yaml: &str) -> Result<String, CompileError> {
    let parsed: RulesYaml = serde_yml::from_str(yaml).map_err(CompileError::Yaml)?;
    emit(&parsed)
}

// --- The codegen pass --------------------------------------------------------

fn emit(rules: &RulesYaml) -> Result<String, CompileError> {
    let default_action = parse_action(&rules.defaults.default_action)?;

    let mut out = String::new();
    writeln!(
        out,
        "// THIS FILE IS GENERATED BY build.rs FROM examples/rules.yaml.\n\
         // Do not edit by hand. Re-run `cargo build` after editing rules.yaml.\n\
         //\n\
         // Phase-1 emit: defaults.default_action and rules[] only. The rest of\n\
         // defaults.* and the entire nat section are echoed as comments so the\n\
         // file is human-scannable; they wire up in subsequent PRs."
    )
    .unwrap();

    // Echo the deferred sections so a reviewer can see what's been parsed
    // but not yet acted on. Comments only, no runtime impact.
    if rules.defaults.upstream_dns.is_some()
        || rules.defaults.flow_trace_sample_accept.is_some()
        || rules.defaults.flow_trace_sample_drop.is_some()
        || rules.defaults.dns_qps_per_client.is_some()
        || rules.defaults.conntrack_timeouts.is_some()
    {
        writeln!(out, "//\n// defaults (parsed but deferred to phase 2+):").unwrap();
        if let Some(v) = &rules.defaults.upstream_dns {
            writeln!(out, "//   upstream_dns = {v}").unwrap();
        }
        if let Some(v) = rules.defaults.flow_trace_sample_accept {
            writeln!(out, "//   flow_trace_sample_accept = {v}").unwrap();
        }
        if let Some(v) = rules.defaults.flow_trace_sample_drop {
            writeln!(out, "//   flow_trace_sample_drop = {v}").unwrap();
        }
        if let Some(v) = rules.defaults.dns_qps_per_client {
            writeln!(out, "//   dns_qps_per_client = {v}").unwrap();
        }
        if rules.defaults.conntrack_timeouts.is_some() {
            writeln!(out, "//   conntrack_timeouts = <set; see rules.yaml>").unwrap();
        }
    }
    if !rules.nat.snat.is_empty() || !rules.nat.dnat.is_empty() {
        writeln!(
            out,
            "//\n// nat (parsed but deferred to phase 3): {} snat entries, {} dnat entries",
            rules.nat.snat.len(),
            rules.nat.dnat.len()
        )
        .unwrap();
    }

    writeln!(out).unwrap();
    // Emit a path that resolves from any callsite that `include!`s us:
    // `src/main.rs` (bin) and `tests/*.rs` reach the types via the lib
    // crate name `thurward`; `src/lib.rs` reaches them via the same path
    // because Cargo lets a crate refer to itself by its public name.
    writeln!(
        out,
        "use thurward::rules::types::{{Action, CidrV4, Direction, PortSpec, Protocol, Rule, StateSpec}};\n"
    )
    .unwrap();

    writeln!(
        out,
        "pub const DEFAULT_ACTION: Action = Action::{};\n",
        action_variant(default_action)
    )
    .unwrap();

    writeln!(out, "pub static RULES: &[Rule] = &[").unwrap();
    for r in &rules.rules {
        emit_rule(&mut out, r)?;
    }
    writeln!(out, "];").unwrap();

    Ok(out)
}

fn emit_rule(out: &mut String, r: &RuleYaml) -> Result<(), CompileError> {
    let id = r.id.as_ref().ok_or(CompileError::MissingId)?.clone();

    // The schema forbids `destination` + `destination_fqdn` together. Belt-
    // and-suspenders: the build.rs run rejects this even if the schema
    // validator hasn't run yet.
    if r.destination.is_some() && r.destination_fqdn.is_some() {
        return Err(CompileError::DestinationConflict { rule_id: id });
    }

    let action = parse_action(&r.action)?;
    let direction = match r.direction.as_deref() {
        Some(s) => parse_direction(s)?,
        None => Direction::Either,
    };
    let source = match r.source.as_deref() {
        Some(s) => Some(parse_cidr(s)?),
        None => None,
    };
    let destination = match r.destination.as_deref() {
        Some(s) => Some(parse_cidr(s)?),
        None => None,
    };
    let destination_port = match r.destination_port.as_ref() {
        Some(PortValue::Single(p)) => PortSpec::Single(*p),
        Some(PortValue::Range(s)) => parse_port_range(s)?,
        None => PortSpec::Any,
    };
    let protocol = match r.protocol.as_deref() {
        Some(s) => parse_protocol(s)?,
        None => Protocol::Any,
    };
    let state = match r.state.as_deref() {
        Some(s) => parse_state(s)?,
        None => StateSpec::Any,
    };

    writeln!(out, "    Rule {{").unwrap();
    writeln!(out, "        id: {},", rust_str(&id)).unwrap();
    writeln!(
        out,
        "        name: {},",
        rust_str(r.name.as_deref().unwrap_or(""))
    )
    .unwrap();
    writeln!(out, "        action: Action::{},", action_variant(action)).unwrap();
    writeln!(
        out,
        "        direction: Direction::{},",
        direction_variant(direction)
    )
    .unwrap();
    writeln!(out, "        source: {},", cidr_expr(source)).unwrap();
    writeln!(out, "        destination: {},", cidr_expr(destination)).unwrap();
    writeln!(
        out,
        "        destination_port: {},",
        port_expr(destination_port)
    )
    .unwrap();
    write!(out, "        destination_fqdn: &[").unwrap();
    if let Some(fqdns) = &r.destination_fqdn {
        for (i, fqdn) in fqdns.iter().enumerate() {
            if i > 0 {
                write!(out, ", ").unwrap();
            }
            write!(out, "{}", rust_str(fqdn)).unwrap();
        }
    }
    writeln!(out, "],").unwrap();
    writeln!(
        out,
        "        protocol: Protocol::{},",
        protocol_variant(protocol)
    )
    .unwrap();
    writeln!(out, "        state: StateSpec::{},", state_variant(state)).unwrap();
    writeln!(out, "    }},").unwrap();
    Ok(())
}

// --- Parsers (YAML string → runtime enum / value) ---------------------------

// These return the runtime types (re-exported via crate root) so callers can
// reason about the *result* of compilation, not just the source code.

use crate::rules::types::{Action, CidrV4, Direction, PortSpec, Protocol, StateSpec};

fn parse_action(s: &str) -> Result<Action, CompileError> {
    match s {
        "accept" => Ok(Action::Accept),
        "deny" => Ok(Action::Deny),
        other => Err(CompileError::BadAction(other.to_string())),
    }
}

fn parse_direction(s: &str) -> Result<Direction, CompileError> {
    match s {
        "egress" => Ok(Direction::Egress),
        "ingress" => Ok(Direction::Ingress),
        other => Err(CompileError::BadDirection(other.to_string())),
    }
}

fn parse_protocol(s: &str) -> Result<Protocol, CompileError> {
    match s {
        "tcp" => Ok(Protocol::Tcp),
        "udp" => Ok(Protocol::Udp),
        "icmp" => Ok(Protocol::Icmp),
        other => Err(CompileError::BadProtocol(other.to_string())),
    }
}

fn parse_state(s: &str) -> Result<StateSpec, CompileError> {
    match s {
        "established" => Ok(StateSpec::Established),
        other => Err(CompileError::BadState(other.to_string())),
    }
}

fn parse_cidr(s: &str) -> Result<CidrV4, CompileError> {
    let (addr_str, prefix_str) = match s.find('/') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, "32"),
    };
    let addr: Ipv4Addr = addr_str
        .parse()
        .map_err(|_| CompileError::BadCidr(s.to_string()))?;
    let prefix_len: u8 = prefix_str
        .parse()
        .map_err(|_| CompileError::BadCidr(s.to_string()))?;
    if prefix_len > 32 {
        return Err(CompileError::BadCidr(s.to_string()));
    }
    Ok(CidrV4 {
        addr: u32::from_be_bytes(addr.octets()),
        prefix_len,
    })
}

fn parse_port_range(s: &str) -> Result<PortSpec, CompileError> {
    let (lo_str, hi_str) = s
        .split_once('-')
        .ok_or_else(|| CompileError::BadPort(s.to_string()))?;
    let lo: u16 = lo_str
        .parse()
        .map_err(|_| CompileError::BadPort(s.to_string()))?;
    let hi: u16 = hi_str
        .parse()
        .map_err(|_| CompileError::BadPort(s.to_string()))?;
    if lo > hi {
        return Err(CompileError::BadPort(s.to_string()));
    }
    Ok(PortSpec::Range(lo, hi))
}

// --- Codegen helpers (runtime enum → Rust source fragment) ------------------

fn action_variant(a: Action) -> &'static str {
    match a {
        Action::Accept => "Accept",
        Action::Deny => "Deny",
    }
}

fn direction_variant(d: Direction) -> &'static str {
    match d {
        Direction::Egress => "Egress",
        Direction::Ingress => "Ingress",
        Direction::Either => "Either",
    }
}

fn protocol_variant(p: Protocol) -> &'static str {
    match p {
        Protocol::Tcp => "Tcp",
        Protocol::Udp => "Udp",
        Protocol::Icmp => "Icmp",
        Protocol::Any => "Any",
    }
}

fn state_variant(s: StateSpec) -> &'static str {
    match s {
        StateSpec::Any => "Any",
        StateSpec::Established => "Established",
    }
}

fn cidr_expr(c: Option<CidrV4>) -> String {
    match c {
        None => "None".to_string(),
        Some(CidrV4 { addr, prefix_len }) => {
            format!("Some(CidrV4 {{ addr: 0x{addr:08x}, prefix_len: {prefix_len} }})")
        }
    }
}

fn port_expr(p: PortSpec) -> String {
    match p {
        PortSpec::Any => "PortSpec::Any".to_string(),
        PortSpec::Single(p) => format!("PortSpec::Single({p})"),
        PortSpec::Range(lo, hi) => format!("PortSpec::Range({lo}, {hi})"),
    }
}

/// Escape a YAML string for embedding as a Rust string literal.
///
/// We use the `r"..."` raw-string form when the input contains no `"` or
/// `\`, and the regular escaped form otherwise. Rule IDs and FQDN patterns
/// are ASCII-printable in practice; this just makes the output prettier.
fn rust_str(s: &str) -> String {
    if !s.contains('"') && !s.contains('\\') {
        format!("r\"{s}\"")
    } else {
        let mut out = String::from("\"");
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
}
