//! A press within the first-mouse window after a toplevel was activated is swallowed unless
//! the element opts in with `data-first-mouse` (the caller checks the DOM).

use std::time::Duration;

/// Whether a press is the one that activated its toplevel. `since_activation` is how long
/// before the press the toplevel became active (its `activated` configure state, or
/// `wl_keyboard.enter`); `None` when it was already active. `window` is the settings key
/// `menus.first_mouse_window_ms` (design/22-SETTINGS.md, default 100 ms). `true` swallows the
/// press, unless the element under it (or an ancestor) carries `data-first-mouse`.
pub(crate) fn first_mouse(since_activation: Option<Duration>, window: Duration) -> bool {
    since_activation.is_some_and(|elapsed| elapsed <= window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_press_inside_the_window_after_activation_is_swallowed() {
        let window = Duration::from_millis(100);
        let cases = [
            ("already active", None, false),
            (
                "activated by this click",
                Some(Duration::from_millis(3)),
                true,
            ),
            ("at the edge", Some(Duration::from_millis(100)), true),
            ("long after", Some(Duration::from_millis(101)), false),
        ];
        for (name, since, want) in cases {
            assert_eq!(first_mouse(since, window), want, "{name}");
        }
    }
}
