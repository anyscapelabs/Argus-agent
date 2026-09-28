use argus_lib::sessions::blocks::esc_attr;

// A multi-line command must survive inside one attribute: no raw newline, no
// raw `>`, or the line-scoped frontend tokenizer drops the whole tag.
#[test]
fn multiline_command_escapes_to_single_line() {
    let out = esc_attr("for f in *; do\necho $f\ndone");

    assert!(!out.contains('\n'), "raw newline breaks the tag: {out}");
    assert!(out.contains("&#10;"), "newline must round-trip: {out}");
}

#[test]
fn redirect_survives_as_entity() {
    let out = esc_attr("grep foo x > out.txt");

    assert!(!out.contains('>'), "raw > ends the tag early: {out}");
    assert!(out.contains("&gt;"), "{out}");
}

#[test]
fn amp_quote_lt_still_escape() {
    let out = esc_attr("a&b\"c<d");

    assert_eq!(out, "a&amp;b&quot;c&lt;d");
}

#[test]
fn carriage_return_escapes() {
    let out = esc_attr("a\rb");

    assert_eq!(out, "a&#13;b");
}
