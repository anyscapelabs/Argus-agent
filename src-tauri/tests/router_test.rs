use argus_lib::gateway::router;
use argus_lib::gateway::schema::Provider;

fn prov(base_url: &str) -> Provider {
    Provider {
        id: "p1".into(),
        name: "Mock".into(),
        compatible: "openai".into(),
        base_url: base_url.into(),
        api_key_ref: None,
        connected: true,
        free: true,
        priority: 0,
        logo_url: None,
        doc_url: None,
    }
}

#[test]
fn a_provider_is_named_in_a_sentence_even_when_it_has_no_name() {
    assert_eq!(router::provider_label("Anthropic", "p1"), "Anthropic (p1)");
    assert_eq!(router::provider_label("Anthropic", ""), "Anthropic");
    assert_eq!(router::provider_label("p1", "p1"), "p1");
    assert_eq!(router::provider_label("  ", "p1"), "Provider p1");
    assert_eq!(router::provider_label("", ""), "The provider");
}

// The local case has to survive an unreachable keyring. Ollama holds no key, so
// a keyring that cannot be read is not this provider's problem — and a headless
// Linux box or a CI runner is exactly where someone runs a local model. This
// is the failure CI found first, because the runner has no secret-service.
#[test]
fn a_local_provider_works_without_a_reachable_keyring() {
    let unreachable = || Err("dbus error: no secret service".to_string());

    for base in [
        "http://127.0.0.1:11434/v1",
        "http://localhost:11434/v1",
        "http://[::1]:11434/v1",
    ] {
        assert_eq!(
            router::decide_key(&prov(base), unreachable()),
            Ok(None),
            "{base} must resolve with no keyring at all"
        );
    }

    // A real key still wins over the fallback — dropping it would send an
    // unauthenticated request to a provider that expects one.
    let key = Ok(Some("sk-local".to_string()));
    assert_eq!(
        router::decide_key(&prov("http://127.0.0.1:11434/v1"), key),
        Ok(Some("sk-local".to_string()))
    );
}

#[test]
fn a_remote_provider_still_fails_closed_when_the_keyring_is_unreachable() {
    let err = router::decide_key(
        &prov("https://api.example.com"),
        Err("dbus error: no secret service".to_string()),
    );
    assert!(err.is_err(), "a remote provider must not proceed unkeyed");
    assert!(err.unwrap_err().contains("could not read the stored key"));
}

#[test]
fn a_remote_provider_without_a_stored_key_says_so() {
    let err = router::decide_key(&prov("https://api.example.com"), Ok(None));
    assert!(err.unwrap_err().contains("no API key stored"));
}

#[test]
fn a_remote_provider_sends_its_stored_key() {
    let key = Ok(Some("sk-abc".to_string()));
    assert_eq!(
        router::decide_key(&prov("https://api.example.com"), key),
        Ok(Some("sk-abc".to_string()))
    );
}

#[test]
fn call_failures_carry_a_kind_for_the_error_card() {
    use argus_lib::gateway::adapters::CallError;

    let err = |status: Option<u16>, msg: &str| CallError {
        status,
        msg: msg.into(),
        retry_after: None,
    };

    assert_eq!(err(Some(401), "unauthorized").kind(), "auth");
    assert_eq!(err(Some(403), "forbidden").kind(), "auth");
    assert_eq!(err(Some(429), "slow down").kind(), "quota");
    assert_eq!(err(Some(500), "boom").kind(), "server");
    assert_eq!(err(Some(400), "bad").kind(), "error");
    assert_eq!(err(None, "connection refused").kind(), "network");
    assert_eq!(
        err(None, "provider accepted the request then went silent: x").kind(),
        "network"
    );
    assert_eq!(err(None, "unknown compatible dialect z").kind(), "error");
}

#[test]
fn plain_failures_classify_without_a_status() {
    assert_eq!(
        router::fail_kind("no API key stored for X — reconnect it in Providers settings"),
        "config"
    );
    assert_eq!(router::fail_kind("provider gone"), "config");
    assert_eq!(
        router::fail_kind("no connected provider serves model m"),
        "config"
    );
    assert_eq!(router::fail_kind("401 Unauthorized"), "auth");
    assert_eq!(
        router::fail_kind("all 10 attempts failed: request was rate limited, slow down"),
        "quota"
    );
    assert_eq!(
        router::fail_kind("model returned an empty reply — try again"),
        "error"
    );
}
