use argus_lib::gateway::schema::WireMsg;
use argus_lib::sessions::chat::{attach_shots, shot_marker};

fn msg(content: &str) -> WireMsg {
    WireMsg {
        role: "user".into(),
        content: content.into(),
        images: vec![],
    }
}

#[test]
fn attach_shots_caps_last_two_and_ignores_outside_paths() {
    let mut msgs = vec![
        msg("screenshot: /d/screenshots/shot-1.png\nimage 1"),
        msg("screenshot: /d/screenshots/shot-2.png"),
        msg("screenshot: /d/screenshots/shot-3.png"),
        msg("no shot here"),
        msg("screenshot: /etc/secret.png"),
    ];

    attach_shots(&mut msgs);

    assert!(msgs[0].images.is_empty());
    assert_eq!(msgs[1].images, vec!["/d/screenshots/shot-2.png"]);
    assert_eq!(msgs[2].images, vec!["/d/screenshots/shot-3.png"]);
    assert!(msgs[3].images.is_empty());
    assert!(msgs[4].images.is_empty());
}

#[test]
fn shot_marker_found_inside_tool_result_wrapper() {
    // Regression: the marker used to be required at line start, but tool
    // results are wrapped — <tool-result ...>screenshot: /path — so no
    // image was ever attached and the model only ever saw file paths.
    let line = "<tool-result tool=\"computer.observe\" status=\"ok\">screenshot: \
                /home/u/.local/share/com.anyscapelabs.argus/screenshots/shot-1789212996301.png\n\
                image 1280x720 of screen</tool-result>";

    assert_eq!(
        shot_marker(line),
        Some(
            "/home/u/.local/share/com.anyscapelabs.argus/screenshots/shot-1789212996301.png".into()
        )
    );

    // a bare marker line still works
    assert_eq!(
        shot_marker("screenshot: /d/screenshots/shot-2.png"),
        Some("/d/screenshots/shot-2.png".into())
    );
}
