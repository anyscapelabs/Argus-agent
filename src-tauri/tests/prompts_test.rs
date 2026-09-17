use argus_lib::prompt::BASE;
use argus_lib::tools::{is_mutating, section};

#[test]
fn base_rules_are_numbered_and_honest_about_permissions() {
    assert!(BASE.contains("1. Act, don't just suggest."), "{BASE}");
    assert!(BASE.contains("Permission modes: ask, never."), "{BASE}");
    assert!(!BASE.contains("allow_once"), "{BASE}");
}

#[test]
fn base_bans_faking_tool_output() {
    assert!(BASE.contains("never fake tool output"), "{BASE}");
}

#[test]
fn section_teaches_the_full_round() {
    let s = section(false);

    assert!(s.contains("A full round looks like this"), "{s}");
    assert!(s.contains("<action tool=\"terminal\">"), "{s}");
    assert!(s.contains("<tool-result tool=\"terminal\""), "{s}");
    assert!(s.contains("action denied by user"), "{s}");
    assert!(s.contains("one per reply"), "{s}");
}

#[test]
fn section_keeps_ref_namespaces_apart() {
    let s = section(false);

    assert!(s.contains("only in browser.* tools"), "{s}");
}

#[test]
fn skill_read_is_listed_and_read_only() {
    let s = section(false);

    assert!(s.contains("skill.read"), "{s}");
    assert!(!is_mutating("skill.read"));
    assert!(is_mutating("terminal"));
    assert!(is_mutating("browser.click"));
    assert!(!is_mutating("browser.read"));
}

#[test]
fn terminal_detach_rule_survives() {
    let s = section(false);

    assert!(s.contains(">/dev/null 2>&1 &"), "{s}");
}
