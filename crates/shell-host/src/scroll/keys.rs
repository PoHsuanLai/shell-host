//! Keyboard scrolling (design/11 §11.3.10, §11.5.4): which keys scroll, by how much, the held
//! arrow's ramp and the spring that ends it. Settled values (R18), so constants.

use keyboard_types::{Key, Modifiers};

use crate::scroll::geom::{Dir, ScrollAxis};

/// One arrow press, px.
pub const LINE_PX: f64 = 40.0;
/// A page keeps this much of the old view in sight, px ...
const PAGE_OVERLAP_PX: f64 = 40.0;
/// ... and moves at least this fraction of the view.
const PAGE_MIN_RATIO: f64 = 0.8;
/// A held arrow reaches its top speed after this long, seconds ...
const HELD_RAMP_S: f64 = 0.2;
/// ... and its top speed is this many steps per second.
const HELD_STEPS_PER_S: f64 = 25.0;
/// On release the spring aims this far ahead of the offset at the release speed, seconds.
const RELEASE_LEAD_S: f64 = 0.1;
/// The spring: mass 1, stiffness 175, damping 20 (zeta 0.756).
const STIFFNESS: f64 = 175.0;
const DAMPING: f64 = 20.0;

/// A key that scrolls, before the target's geometry is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollKey {
    /// An arrow: one line.
    Line(ScrollAxis, Dir),
    /// Page Up / Page Down, Space / Shift+Space: one page, vertically.
    Page(Dir),
    /// Home / End, Ctrl+Up / Ctrl+Down: to an edge, vertically.
    Edge(Dir),
}

impl ScrollKey {
    /// The axis the key moves along.
    pub fn axis(self) -> ScrollAxis {
        match self {
            ScrollKey::Line(axis, _) => axis,
            ScrollKey::Page(_) | ScrollKey::Edge(_) => ScrollAxis::Y,
        }
    }

    /// Whether holding the key runs the held ramp (arrows only).
    pub fn holds(self) -> bool {
        matches!(self, ScrollKey::Line(..))
    }
}

/// The scroll key `key` with `mods` is, if any. Alt and Super combinations are shortcuts, never
/// scrolls; Ctrl only turns Up/Down into Home/End.
pub fn scroll_key(key: &Key, mods: Modifiers) -> Option<ScrollKey> {
    if mods.intersects(Modifiers::ALT | Modifiers::META | Modifiers::SUPER | Modifiers::HYPER) {
        return None;
    }
    let ctrl = mods.contains(Modifiers::CONTROL);
    let shift = mods.contains(Modifiers::SHIFT);
    match (key, ctrl, shift) {
        (Key::ArrowUp, true, false) | (Key::Home, _, false) => Some(ScrollKey::Edge(Dir::Neg)),
        (Key::ArrowDown, true, false) | (Key::End, _, false) => Some(ScrollKey::Edge(Dir::Pos)),
        (Key::ArrowUp, false, false) => Some(ScrollKey::Line(ScrollAxis::Y, Dir::Neg)),
        (Key::ArrowDown, false, false) => Some(ScrollKey::Line(ScrollAxis::Y, Dir::Pos)),
        (Key::ArrowLeft, false, false) => Some(ScrollKey::Line(ScrollAxis::X, Dir::Neg)),
        (Key::ArrowRight, false, false) => Some(ScrollKey::Line(ScrollAxis::X, Dir::Pos)),
        (Key::PageUp, false, false) => Some(ScrollKey::Page(Dir::Neg)),
        (Key::PageDown, false, false) => Some(ScrollKey::Page(Dir::Pos)),
        (Key::Character(c), false, shifted) if c == " " => Some(ScrollKey::Page(match shifted {
            true => Dir::Neg,
            false => Dir::Pos,
        })),
        _ => None,
    }
}

/// The page step for a view `viewport` px long: `max(0.8 v, v - 40)`.
pub fn page(viewport: f64) -> f64 {
    (PAGE_MIN_RATIO * viewport).max(viewport - PAGE_OVERLAP_PX)
}

/// A held arrow's speed `t` s after the ramp started, for a signed `step` (px per press).
pub fn held_speed(step: f64, t: f64) -> f64 {
    (t / HELD_RAMP_S).clamp(0.0, 1.0) * HELD_STEPS_PER_S * step
}

/// How far a held arrow has moved `t` s after the ramp started (the integral of its speed).
pub fn held_travel(step: f64, t: f64) -> f64 {
    let top = HELD_STEPS_PER_S * step;
    let t = t.max(0.0);
    if t <= HELD_RAMP_S {
        top * t * t / (2.0 * HELD_RAMP_S)
    } else {
        top * (HELD_RAMP_S / 2.0 + (t - HELD_RAMP_S))
    }
}

/// Where the release spring aims: the offset carried on at the release speed for 100 ms.
pub fn release_target(x: f64, v: f64) -> f64 {
    x + v * RELEASE_LEAD_S
}

/// The spring's displacement from its target and its velocity `t` s after starting at
/// displacement `e0` with velocity `v0` (`x'' = -175 e - 20 x'`, under-damped).
pub fn spring(e0: f64, v0: f64, t: f64) -> (f64, f64) {
    let decay = DAMPING / 2.0;
    let wd = (STIFFNESS - decay * decay).sqrt();
    let b = (v0 + decay * e0) / wd;
    let (sin, cos) = (wd * t).sin_cos();
    let envelope = (-decay * t).exp();
    let e = envelope * (e0 * cos + b * sin);
    let v = envelope * ((b * wd - decay * e0) * cos - (e0 * wd + decay * b) * sin);
    (e, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scroll_keys() {
        let none = Modifiers::empty();
        let space = Key::Character(" ".into());
        let cases = [
            (
                "down",
                Key::ArrowDown,
                none,
                Some(ScrollKey::Line(ScrollAxis::Y, Dir::Pos)),
            ),
            (
                "up",
                Key::ArrowUp,
                none,
                Some(ScrollKey::Line(ScrollAxis::Y, Dir::Neg)),
            ),
            (
                "right",
                Key::ArrowRight,
                none,
                Some(ScrollKey::Line(ScrollAxis::X, Dir::Pos)),
            ),
            (
                "page down",
                Key::PageDown,
                none,
                Some(ScrollKey::Page(Dir::Pos)),
            ),
            (
                "space",
                space.clone(),
                none,
                Some(ScrollKey::Page(Dir::Pos)),
            ),
            (
                "shift space",
                space,
                Modifiers::SHIFT,
                Some(ScrollKey::Page(Dir::Neg)),
            ),
            ("home", Key::Home, none, Some(ScrollKey::Edge(Dir::Neg))),
            (
                "ctrl down",
                Key::ArrowDown,
                Modifiers::CONTROL,
                Some(ScrollKey::Edge(Dir::Pos)),
            ),
            ("alt down", Key::ArrowDown, Modifiers::ALT, None),
            ("shift down selects", Key::ArrowDown, Modifiers::SHIFT, None),
            ("super end", Key::End, Modifiers::META, None),
            ("a letter", Key::Character("j".into()), none, None),
        ];
        for (name, key, mods, want) in cases {
            assert_eq!(scroll_key(&key, mods), want, "{name}");
        }
    }

    #[test]
    fn a_page_is_the_view_less_40_or_80_percent() {
        assert_eq!(page(900.0), 860.0);
        assert_eq!(page(100.0), 80.0);
    }

    #[test]
    fn a_held_line_reaches_1000_px_s_in_200_ms() {
        assert_eq!(held_speed(LINE_PX, 0.1), 500.0);
        assert_eq!(held_speed(LINE_PX, 0.2), 1000.0);
        assert_eq!(held_speed(-LINE_PX, 1.0), -1000.0);
        assert!((held_travel(LINE_PX, 0.2) - 100.0).abs() < 1e-9);
        assert!((held_travel(LINE_PX, 1.0) - 900.0).abs() < 1e-9);
    }

    #[test]
    fn the_release_spring_follows_11_8_item_11() {
        // Released at 1000 px/s: target 100 px ahead, e(t) = -100 e^(-10t) cos(8.66 t).
        for ms in [0, 50, 100, 200, 400] {
            let t = f64::from(ms) / 1000.0;
            let (e, _) = spring(-100.0, 1000.0, t);
            let want = -100.0 * (-10.0 * t).exp() * (8.660_254 * t).cos();
            assert!((e - want).abs() < 1e-3, "{ms} ms: {e} vs {want}");
        }
        let (e, v) = spring(-100.0, 1000.0, 0.0);
        assert_eq!((e, v), (-100.0, 1000.0));
        let (e, v) = spring(-100.0, 1000.0, 0.6);
        assert!(e.abs() < 0.5 && v.abs() < 5.0, "{e} {v}");
    }
}
