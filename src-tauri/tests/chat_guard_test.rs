use argus_lib::sessions::chat::{fakes_output, has_faux_sandbox};

#[test]
fn faux_sandbox_flags_model_written_blocks() {
    assert!(has_faux_sandbox(
        "Now let me run it.\n<sandbox command=\"make test\" profile=\"offline\"></sandbox>"
    ));
    assert!(!has_faux_sandbox("All done, here is the summary."));
    assert!(!has_faux_sandbox(
        "<action tool=\"terminal\">{\"command\":\"make test\"}</action>"
    ));
    assert!(!has_faux_sandbox(""));
}

#[test]
fn fakes_output_still_flags_terminal_and_browser() {
    assert!(fakes_output(
        "Pushing now.\n<terminal id=\"a8\" command=\"git push\"></terminal>"
    ));
    assert!(fakes_output(
        "Opening it.\n<browser-action url=\"https://x\">"
    ));
    assert!(!fakes_output("Pushed and verified."));
}
