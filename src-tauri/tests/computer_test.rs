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
