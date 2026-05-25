//! thurward bin — placeholder until PR B/C land the dataplane.
//!
//! PR A's binary just demonstrates that the compiled rule table is
//! reachable from the bin entry. It prints the rule count and the
//! default action, then exits. PR B/C replace this `main` with the
//! Hermit boot path that spawns RX poll threads.

include!(concat!(env!("OUT_DIR"), "/rules_table.rs"));

fn main() {
    println!(
        "thurward: {} rule(s) compiled in; default action = {:?}",
        RULES.len(),
        DEFAULT_ACTION
    );
    for rule in RULES {
        println!("  {}  ({:?})", rule.id, rule.action);
    }
}
