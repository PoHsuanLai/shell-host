//! A press within the first-mouse window after a toplevel was activated is swallowed unless
//! the element opts in with `data-first-mouse` (the caller checks the DOM).

use std::time::Duration;

/// Whether a press reaches the document or is swallowed as the click that activated a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FirstMouse {
    Deliver,
    /// Swallow it, unless the element under it (or an ancestor) carries `data-first-mouse`.
    Swallow,
}

/// Whether a press is the one that activated its toplevel. `since_activation` is how long
/// before the press the toplevel became active (its `activated` configure state, or
/// `wl_keyboard.enter`); `None` when it was already active. `window` is the settings key
/// `menus.first_mouse_window_ms` (design/22-SETTINGS.md, default 100 ms).
pub(crate) fn first_mouse(since_activation: Option<Duration>, window: Duration) -> FirstMouse {
    match since_activation {
        Some(elapsed) if elapsed <= window => FirstMouse::Swallow,
        _ => FirstMouse::Deliver,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_press_inside_the_window_after_activation_is_swallowed() {
        let window = Duration::from_millis(100);
        let cases = [
            ("already active", None, FirstMouse::Deliver),
            (
                "activated by this click",
                Some(Duration::from_millis(3)),
                FirstMouse::Swallow,
            ),
            (
                "at the edge",
                Some(Duration::from_millis(100)),
                FirstMouse::Swallow,
            ),
            (
                "long after",
                Some(Duration::from_millis(101)),
                FirstMouse::Deliver,
            ),
        ];
        for (name, since, want) in cases {
            assert_eq!(first_mouse(since, window), want, "{name}");
        }
    }
}
