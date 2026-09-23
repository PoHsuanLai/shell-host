//! Regions: describing a blur or input region, and decomposing a rounded rectangle into the
//! axis-aligned strips a `wl_region` can express.
//!
//! Wayland regions are unions of rectangles; there is no primitive for a rounded corner. A
//! rounded rectangle is therefore approximated as one 1-logical-pixel-tall strip per row of
//! each corner (inset by how far the circular corner has eaten into that row), plus the
//! untouched middle band. `rounded_strips` is the pure geometry; turning the result into a real
//! `wl_region` (via `wl_compositor.create_region` + repeated `wl_region.add`) is an effect that
//! belongs to the caller.

/// A logical-pixel coordinate or extent, surface-local.
pub type Px = i32;

/// An axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: Px,
    pub y: Px,
    pub width: Px,
    pub height: Px,
}

impl Rect {
    /// A rectangle with no area, at the origin.
    pub const ZERO: Rect = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// `true` for a rectangle with positive width and height.
    pub fn has_area(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// A rectangle with a single uniform corner radius, clamped so opposite corners never overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundedRect {
    pub rect: Rect,
    pub radius: Px,
}

impl RoundedRect {
    /// The radius actually used: clamped to be non-negative and to at most half of each side,
    /// so that the four corner arcs never overlap.
    pub fn effective_radius(self) -> Px {
        self.radius
            .max(0)
            .min(self.rect.width / 2)
            .min(self.rect.height / 2)
    }
}

/// A DOM element's `id` attribute, resolved to a `Rect` after layout by the caller
/// (`get_element_by_id(id.as_str())` + its bounding rect).
///
/// A string, not a hash: Blitz finds an element only by its `id` attribute string
/// (`BaseDocument::get_element_by_id(&str)`), so anything else would need an interning table to
/// get the string back (FINDINGS.md F3, resolved by this newtype).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementId(String);

impl ElementId {
    /// The element whose `id` attribute is `id`.
    pub fn new(id: impl Into<String>) -> ElementId {
        ElementId(id.into())
    }

    /// The `id` attribute this names.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a blur or input region is described before it is resolved to concrete geometry and sent
/// to the compositor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionSpec {
    /// No region: blur off, or nothing hit-testable.
    Empty,
    /// The whole surface.
    Whole,
    /// An explicit set of rounded rectangles, already in surface-local coordinates.
    Rects(Vec<RoundedRect>),
    /// The bounding rect of a DOM element, inflated and rounded, resolved after layout.
    Element {
        id: ElementId,
        radius: Px,
        inflate: Px,
    },
}

/// Decompose a rounded rectangle into the axis-aligned strips whose union approximates it.
///
/// Every returned strip lies entirely inside the true rounded rectangle (no strip pixel is
/// outside the rounded shape), and the amount trimmed off the corners is never more than the
/// rectangle's `effective_radius()`. A radius of zero or a degenerate rectangle returns the
/// rectangle itself (zero or one strip).
pub fn rounded_strips(rr: RoundedRect) -> Vec<Rect> {
    let Rect {
        x,
        y,
        width,
        height,
    } = rr.rect;
    if !rr.rect.has_area() {
        return Vec::new();
    }
    let r = rr.effective_radius();
    if r <= 0 {
        return vec![rr.rect];
    }

    let mut strips = Vec::with_capacity(2 * r as usize + 1);

    // One row per unit of the corner band, top then bottom. `row` counts down from the flat
    // edge of the rect into the corner (row 0 is the outermost row, row r-1 the row closest to
    // the straight sides).
    for row in 0..r {
        let inset = corner_inset(r, row);
        if width - 2 * inset <= 0 {
            continue;
        }
        strips.push(Rect {
            x: x + inset,
            y: y + row,
            width: width - 2 * inset,
            height: 1,
        });
    }

    let middle_height = height - 2 * r;
    if middle_height > 0 {
        strips.push(Rect {
            x,
            y: y + r,
            width,
            height: middle_height,
        });
    }

    for row in 0..r {
        let inset = corner_inset(r, row);
        if width - 2 * inset <= 0 {
            continue;
        }
        // `row` still counts down from the flat (here, bottom) edge, so the outermost bottom
        // row (row 0) is the rect's last row, mirroring the top band.
        let strip_y = y + height - 1 - row;
        strips.push(Rect {
            x: x + inset,
            y: strip_y,
            width: width - 2 * inset,
            height: 1,
        });
    }

    strips
}

/// How far a corner strip at `row` (0 = outermost, the row against the flat edge) must be
/// inset from the straight sides of a `radius`-`r` corner so every pixel in the strip lies
/// inside the quarter circle.
///
/// The circle is centred on the corner square (at `(r, r)`, continuous coordinates); pixel
/// `(x, row)` within the square is inside it when its *centre*, `(x + 0.5, row + 0.5)`, is
/// within `r` of that centre. Solving `(r - x - 0.5)^2 + dy^2 <= r^2` for the smallest inside
/// `x` (where `dy = r - row - 0.5` is the row centre's distance from the centre row) gives the
/// inset directly; rounding it *up* keeps every pixel in the strip on the inside of the true
/// boundary, at the cost of shaving a little coverage off pixels the circle only barely
/// contains.
fn corner_inset(r: Px, row: Px) -> Px {
    let r = f64::from(r);
    let dy = r - f64::from(row) - 0.5;
    let reach_sq = (r * r - dy * dy).max(0.0);
    let bound = r - 0.5 - reach_sq.sqrt();
    bound.max(0.0).ceil() as Px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_ids_compare_by_the_name_they_were_built_from() {
        assert_eq!(ElementId::new("dock-pill"), ElementId::new("dock-pill"));
        assert_eq!(ElementId::new("dock-pill").as_str(), "dock-pill");
        assert_ne!(ElementId::new("dock-pill"), ElementId::new("dock-hit"));
        // `impl Into<String>` accepts both a borrowed and an owned string.
        assert_eq!(
            ElementId::new("dock-pill"),
            ElementId::new("dock-pill".to_owned())
        );
    }

    #[test]
    fn zero_radius_is_the_rect_itself() {
        let rr = RoundedRect {
            rect: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 5,
            },
            radius: 0,
        };
        assert_eq!(rounded_strips(rr), vec![rr.rect]);
    }

    #[test]
    fn degenerate_rect_has_no_strips() {
        let rr = RoundedRect {
            rect: Rect::ZERO,
            radius: 4,
        };
        assert!(rounded_strips(rr).is_empty());
    }

    #[test]
    fn radius_is_clamped_to_half_the_shorter_side() {
        // 10x6 rect: half-height is 3, so a requested radius of 100 clamps to 3.
        let rr = RoundedRect {
            rect: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 6,
            },
            radius: 100,
        };
        assert_eq!(rr.effective_radius(), 3);
    }

    #[test]
    fn every_corner_inset_is_within_the_radius() {
        for r in 1..40 {
            for row in 0..r {
                let inset = corner_inset(r, row);
                assert!((0..=r).contains(&inset), "r={r} row={row} inset={inset}");
            }
        }
    }
}
