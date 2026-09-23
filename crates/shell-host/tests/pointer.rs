//! Pointer events reaching a real document: the move-before-down, leave and wheel rules from
//! `input::pointer`, applied to a `SurfaceDocument` built with the host's UA stylesheet. H2
//! input fills these once `dom::document` exists.

/// With the UA stylesheet's sized root, a hover over an in-flow dock icon hovers the icon, not
/// `main` (SPIKE.md S1 finding 2).
#[test]
#[ignore = "H2 fill"]
fn hover_reaches_in_flow_content() {
    todo!()
}

/// A press with no motion before it still clicks the element under it: the synthesized move
/// sets the hover target first.
#[test]
#[ignore = "H2 fill"]
fn a_press_without_prior_motion_clicks_its_target() {
    todo!()
}

/// Leaving clears `:hover` and parks the pointer at (-1, -1): the hovered element's hover
/// style is gone after the next resolve.
#[test]
#[ignore = "H2 fill"]
fn leaving_clears_hover() {
    todo!()
}

/// One wheel notch scrolls an overflowing element by Blitz's line height times one, and a
/// touchpad delta scrolls by exactly its pixels.
#[test]
#[ignore = "H2 fill"]
fn wheel_notches_scroll_by_lines_and_touchpads_by_pixels() {
    todo!()
}

/// A press within the first-mouse window after activation is swallowed unless the element
/// carries `data-first-mouse`.
#[test]
#[ignore = "H2 fill"]
fn the_activating_click_is_swallowed_unless_opted_in() {
    todo!()
}
