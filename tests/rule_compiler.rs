//! Integration tests for the rule compiler.
//!
//! Each test parses a fixture, compiles to Rust source, and checks
//! observable properties. We deliberately don't snapshot the literal
//! output: the goal is the *semantics* of the generated module, not its
//! whitespace.

use thurward::rules::compiler::{CompileError, compile};

const EXAMPLES_RULES_YAML: &str = include_str!("../examples/rules.yaml");

#[test]
fn examples_rules_yaml_compiles() {
    let src = compile(EXAMPLES_RULES_YAML).expect("examples/rules.yaml must compile");
    // The canonical example has three rules; preserve that as a regression guard.
    assert_eq!(rules_in(&src), 3);
    assert!(src.contains("pub const DEFAULT_ACTION: Action = Action::Deny;"));
    // Each rule id appears verbatim in the emitted module.
    for id in ["allow-dns-out", "allow-github-https", "allow-established"] {
        assert!(
            src.contains(&format!("id: r\"{id}\"")),
            "missing id {id} in:\n{src}"
        );
    }
}

#[test]
fn minimal_yaml_emits_one_unconstrained_rule() {
    let src = compile(include_str!("fixtures/minimal.yaml")).expect("minimal compiles");
    assert_eq!(rules_in(&src), 1);
    // No source CIDR, no destination, no port, no protocol → everything `None`/`Any`.
    assert!(src.contains("source: None"));
    assert!(src.contains("destination: None"));
    assert!(src.contains("destination_port: PortSpec::Any"));
    assert!(src.contains("protocol: Protocol::Any"));
    assert!(src.contains("direction: Direction::Either"));
}

#[test]
fn port_range_yaml_emits_port_range_variant() {
    let src = compile(include_str!("fixtures/port_range.yaml")).expect("port_range compiles");
    assert!(src.contains("destination_port: PortSpec::Range(1024, 65535)"));
}

#[test]
fn default_allow_yaml_flips_default_action() {
    let src = compile(include_str!("fixtures/default_allow.yaml")).expect("default_allow compiles");
    assert!(src.contains("pub const DEFAULT_ACTION: Action = Action::Accept;"));
    assert!(src.contains("action: Action::Deny"));
}

#[test]
fn host_route_yaml_yields_prefix_32() {
    let src = compile(include_str!("fixtures/host_route.yaml")).expect("host_route compiles");
    // 198.51.100.42 = 0xC633642A
    assert!(
        src.contains("0xc633642a"),
        "missing host route addr in:\n{src}"
    );
    assert!(src.contains("prefix_len: 32"));
}

#[test]
fn rule_without_id_is_rejected() {
    let yaml = "rules:\n  - action: accept\n";
    match compile(yaml) {
        Err(CompileError::MissingId) => {}
        other => panic!("expected MissingId, got {other:?}"),
    }
}

#[test]
fn bad_cidr_is_rejected() {
    let yaml = "rules:\n  - id: x\n    action: accept\n    source: 999.999.999.0/24\n";
    match compile(yaml) {
        Err(CompileError::BadCidr(s)) => assert_eq!(s, "999.999.999.0/24"),
        other => panic!("expected BadCidr, got {other:?}"),
    }
}

#[test]
fn bad_action_is_rejected() {
    let yaml = "rules:\n  - id: x\n    action: maybe\n";
    match compile(yaml) {
        Err(CompileError::BadAction(s)) => assert_eq!(s, "maybe"),
        other => panic!("expected BadAction, got {other:?}"),
    }
}

#[test]
fn destination_and_fqdn_together_is_rejected() {
    let yaml = "\
rules:
  - id: ambiguous
    action: accept
    destination: 10.0.0.5
    destination_fqdn: [example.com]
    protocol: tcp
";
    match compile(yaml) {
        Err(CompileError::DestinationConflict { rule_id }) => assert_eq!(rule_id, "ambiguous"),
        other => panic!("expected DestinationConflict, got {other:?}"),
    }
}

#[test]
fn port_range_with_inverted_bounds_is_rejected() {
    let yaml = "\
rules:
  - id: inverted
    action: accept
    destination_port: \"5000-100\"
    protocol: tcp
";
    match compile(yaml) {
        Err(CompileError::BadPort(s)) => assert_eq!(s, "5000-100"),
        other => panic!("expected BadPort, got {other:?}"),
    }
}

// --- Helpers -----------------------------------------------------------------

/// Count `Rule {` literals in the generated source. Cheap proxy for
/// `RULES.len()` without compiling the emitted code.
fn rules_in(src: &str) -> usize {
    src.matches("Rule {").count()
}

// --- Runtime-type unit tests -------------------------------------------------

mod cidr_contains {
    use thurward::rules::types::CidrV4;

    fn cidr(addr: u32, prefix_len: u8) -> CidrV4 {
        CidrV4 { addr, prefix_len }
    }

    #[test]
    fn slash_24_contains_all_24_addresses() {
        let net = cidr(0x0a000000, 24); // 10.0.0.0/24
        assert!(net.contains(0x0a000000)); // 10.0.0.0
        assert!(net.contains(0x0a0000ff)); // 10.0.0.255
        assert!(!net.contains(0x0a000100)); // 10.0.1.0
        assert!(!net.contains(0x09ffffff)); // just below
    }

    #[test]
    fn slash_32_is_exact_host() {
        let net = cidr(0x0a000001, 32);
        assert!(net.contains(0x0a000001));
        assert!(!net.contains(0x0a000002));
    }

    #[test]
    fn slash_zero_matches_everything() {
        let net = cidr(0, 0);
        assert!(net.contains(0));
        assert!(net.contains(u32::MAX));
        assert!(net.contains(0xdeadbeef));
    }
}
