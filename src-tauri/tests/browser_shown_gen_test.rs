// The shown-generation marker: how a tool result is recognised as carrying a
// snapshot number, and how the ref table ages out between snapshots.

use argus_lib::tools::browser::{shown_gen_in, RefEntry, RefTable};

fn table(gen: u64, rows: &[(&str, &str, &str)]) -> RefTable {
    RefTable {
        gen,
        items: rows
            .iter()
            .map(|(kind, label, path)| RefEntry {
                kind: kind.to_string(),
                label: label.to_string(),
                path: path.to_string(),
            })
            .collect(),
        recovery_gen: None,
    }
}

#[test]
fn tokenless_steady_state_proceeds() {
    let mut t = table(5, &[("button", "Go", "b1"), ("input", "Name", "i1")]);
    assert!(t.stale_for_tokenless(0, Some(5)).is_none());
    assert!(t.stale_for_tokenless(1, Some(5)).is_none());
    assert!(t.recovery_gen.is_none());
}

#[test]
fn tokenless_drift_returns_bounded_recovery() {
    let mut t = table(5, &[("button", "Overview", "ov"), ("button", "Go", "b2")]);
    let first = t
        .stale_for_tokenless(0, Some(4))
        .expect("drifted token-less ref must be rejected");
    assert!(first.contains("stale ref 0 from snapshot 4"));
    assert!(first.contains("snapshot 5 is current"));
    assert!(first.contains("Elements (snapshot 5)"));
    assert!(first.contains("Choose the replacement ref"));
    assert_eq!(t.recovery_gen, Some(5));

    let second = t
        .stale_for_tokenless(0, Some(4))
        .expect("repeat must still be rejected");
    assert!(second.contains("stale ref"));
    assert!(
        !second.contains("Elements (snapshot"),
        "repeat must not smuggle another snapshot: {second}"
    );
    assert!(second.contains("run browser.read"));
}

#[test]
fn tokenless_without_presentation_proceeds() {
    let mut t = table(5, &[("button", "Go", "b1")]);
    assert!(t.stale_for_tokenless(0, None).is_none());
    assert!(t.recovery_gen.is_none());
}

#[test]
fn tokenless_unknown_ref_under_drift_falls_through() {
    let mut t = table(5, &[("button", "Go", "b1")]);
    assert!(t.stale_for_tokenless(9, Some(4)).is_none());
    assert!(t.recovery_gen.is_none());
}

#[test]
fn explicit_resolution_keeps_classic_shape() {
    let t = table(5, &[("button", "Go", "b1")]);
    let err = t
        .resolve(0, Some(4))
        .expect_err("old explicit snapshot must be stale");
    assert!(err.contains("stale ref 0 from snapshot 4"));
    assert!(!err.contains("Choose the replacement"));
}

#[test]
fn shown_gen_extraction() {
    assert_eq!(
        shown_gen_in(
            "url u\ntitle t\n---\nbody\n---\nElements (snapshot 12):\n[0] button \"Go\"\n"
        ),
        Some(12)
    );
    assert_eq!(
        shown_gen_in("\nElements (snapshot 7): (snapshot failed)\n"),
        Some(7)
    );
    assert_eq!(shown_gen_in("browser closed"), None);
    assert_eq!(shown_gen_in(""), None);
}
