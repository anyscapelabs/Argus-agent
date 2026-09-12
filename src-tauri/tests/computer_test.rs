use argus_lib::tools::computer::atspi::ref_index;
use argus_lib::tools::computer::x11::scale_coords;

#[test]
fn coords_scale_screenshot_to_screen() {
    assert_eq!(
        scale_coords(320.0, 180.0, 640, 360, 1280, 720),
        Some((640, 360))
    );
    assert_eq!(scale_coords(0.0, 0.0, 640, 360, 1280, 720), Some((0, 0)));
}

#[test]
fn coords_reject_outside_bounds() {
    assert_eq!(scale_coords(700.0, 10.0, 640, 360, 1280, 720), None);
    assert_eq!(scale_coords(-1.0, 10.0, 640, 360, 1280, 720), None);
    assert_eq!(scale_coords(10.0, 360.0, 640, 360, 1280, 720), None);
}

#[test]
fn ref_index_is_one_based() {
    assert_eq!(ref_index(1, 3), Some(0));
    assert_eq!(ref_index(3, 3), Some(2));
}

#[test]
fn ref_index_rejects_unknown() {
    assert_eq!(ref_index(0, 3), None);
    assert_eq!(ref_index(-2, 3), None);
    assert_eq!(ref_index(4, 3), None);
    assert_eq!(ref_index(1, 0), None);
}

#[test]
fn repeated_trips_on_third_identical_action() {
    use argus_lib::sessions::chat::repeated;

    let a = ("computer.click".to_string(), "{\"x\":1}".to_string());
    let b = ("computer.screen".to_string(), "{}".to_string());
    let seen = vec![a.clone(), a.clone()];

    assert!(repeated(&seen, &a));
    assert!(!repeated(&vec![a.clone()], &a));
    assert!(!repeated(&vec![a.clone(), b.clone(), a.clone()], &a));
    assert!(!repeated(&vec![], &a));
}

#[test]
fn find_desktop_matches_stem_then_name() {
    use argus_lib::tools::computer::x11::find_desktop;

    let dir = std::env::temp_dir().join(format!("argus-launch-{}", std::process::id()));
    let apps = dir.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(
        apps.join("argustestapp.desktop"),
        "[Desktop Entry]\nName=Argus Test App\n",
    )
    .unwrap();
    std::fs::write(
        apps.join("other.desktop"),
        "[Desktop Entry]\nName=Other Thing\n",
    )
    .unwrap();

    let prev = std::env::var("XDG_DATA_DIRS").ok();
    std::env::set_var("XDG_DATA_DIRS", &dir);

    assert_eq!(
        find_desktop("argustestapp").unwrap(),
        "argustestapp.desktop"
    );
    assert_eq!(find_desktop("test app").unwrap(), "argustestapp.desktop");
    assert!(find_desktop("no-such-app-xyz").is_err());

    match prev {
        Some(v) => std::env::set_var("XDG_DATA_DIRS", v),
        None => std::env::remove_var("XDG_DATA_DIRS"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
