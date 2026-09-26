use argus_lib::gateway::router;

#[test]
fn a_provider_is_named_in_a_sentence_even_when_it_has_no_name() {
    assert_eq!(router::provider_label("Anthropic", "p1"), "Anthropic (p1)");
    assert_eq!(router::provider_label("Anthropic", ""), "Anthropic");
    assert_eq!(router::provider_label("p1", "p1"), "p1");
    assert_eq!(router::provider_label("  ", "p1"), "Provider p1");
    assert_eq!(router::provider_label("", ""), "The provider");
}
