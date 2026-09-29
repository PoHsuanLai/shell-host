//! The engine's vocabulary: which scroller, which axis, which way, and the geometry the host
//! reads from the laid-out document before every step. Plain values; nothing here touches Blitz.

/// A scroll offset or distance in logical (CSS) pixels.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Px(pub f64);

/// A scroll axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollAxis {
    X,
    Y,
}

/// A direction along an axis: toward offset 0 or toward the maximum offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Neg,
    Pos,
}

impl Dir {
    /// The direction of a signed offset change; `None` for no change.
    pub fn of(delta: f64) -> Option<Dir> {
        if delta > 0.0 {
            Some(Dir::Pos)
        } else if delta < 0.0 {
            Some(Dir::Neg)
        } else {
            None
        }
    }

    /// `+1` or `-1`.
    pub fn sign(self) -> f64 {
        match self {
            Dir::Neg => -1.0,
            Dir::Pos => 1.0,
        }
    }
}

/// What scrolls: the document viewport, or an element (a Blitz `NodeId`, versioned, so a removed node never aliases a new one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scroller {
    Viewport,
    Node(u64),
}

/// Whether a scroller stretches past its edges under the fingers (design/11 §11.3.7): only a
/// container that opts in with `data-overscroll="band"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elastic {
    Elastic,
    Rigid,
}

/// One scroller's geometry on one axis, read from the laid-out document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geom {
    /// The offset written now (raw: outside `0..=max` while stretched).
    pub offset: Px,
    /// The largest in-range offset (content less viewport; 0 when the content fits).
    pub max: Px,
    /// The scroller's own extent on the axis (its viewport).
    pub viewport: Px,
    pub elastic: Elastic,
}

impl Geom {
    /// Whether the scroller can move from its current offset toward `dir`.
    pub fn can_move(&self, dir: Dir) -> bool {
        const EPSILON: f64 = 0.5;
        match dir {
            Dir::Neg => self.offset.0 > EPSILON,
            Dir::Pos => self.offset.0 < self.max.0 - EPSILON,
        }
    }

    /// `x` clamped into `0..=max`.
    pub fn clamp(&self, x: f64) -> f64 {
        x.clamp(0.0, self.max.0.max(0.0))
    }

    /// The edge offset on the `dir` side.
    pub fn edge(&self, dir: Dir) -> f64 {
        match dir {
            Dir::Neg => 0.0,
            Dir::Pos => self.max.0.max(0.0),
        }
    }
}

/// The scroller a gesture moves, chosen once at its start and kept through its momentum and
/// rebound (design/11 §11.3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Latch {
    pub scroller: Scroller,
    pub axis: ScrollAxis,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(offset: f64, max: f64) -> Geom {
        Geom {
            offset: Px(offset),
            max: Px(max),
            viewport: Px(100.0),
            elastic: Elastic::Rigid,
        }
    }

    #[test]
    fn a_scroller_moves_only_away_from_the_edge_it_is_at() {
        let cases = [
            ("top, down", geom(0.0, 500.0), Dir::Pos, true),
            ("top, up", geom(0.0, 500.0), Dir::Neg, false),
            ("bottom, down", geom(500.0, 500.0), Dir::Pos, false),
            ("middle, up", geom(250.0, 500.0), Dir::Neg, true),
            ("fits, down", geom(0.0, 0.0), Dir::Pos, false),
            (
                "sub-pixel from the bottom",
                geom(499.8, 500.0),
                Dir::Pos,
                false,
            ),
        ];
        for (name, g, dir, want) in cases {
            assert_eq!(g.can_move(dir), want, "{name}");
        }
    }

    #[test]
    fn direction_and_clamp() {
        assert_eq!(Dir::of(3.0), Some(Dir::Pos));
        assert_eq!(Dir::of(-0.1), Some(Dir::Neg));
        assert_eq!(Dir::of(0.0), None);
        let g = geom(0.0, 500.0);
        assert_eq!(g.clamp(-5.0), 0.0);
        assert_eq!(g.clamp(600.0), 500.0);
        assert_eq!(g.edge(Dir::Pos), 500.0);
    }
}
