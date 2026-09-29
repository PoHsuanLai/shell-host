//! The frame, the surface around it and the coordinates between them.

use super::Inset;
use crate::input::pointer::PointerPos;
use crate::region::Rect;
use crate::scale::{Scale, Size, logical_to_physical};

/// The surface around a frame of `frame`.
pub(crate) fn surface_of(frame: Size, inset: Inset) -> Size {
    Size {
        width: frame.width + 2 * inset.0,
        height: frame.height + 2 * inset.0,
    }
}

/// The frame inside a surface of `surface`.
pub(crate) fn frame_of(surface: Size, inset: Inset) -> Size {
    Size {
        width: (surface.width - 2 * inset.0).max(0),
        height: (surface.height - 2 * inset.0).max(0),
    }
}

/// `xdg_surface.set_window_geometry` for a surface of `surface`: the frame, in surface
/// coordinates.
pub(crate) fn window_geometry(surface: Size, inset: Inset) -> Rect {
    let frame = frame_of(surface, inset);
    Rect {
        x: inset.0,
        y: inset.0,
        width: frame.width,
        height: frame.height,
    }
}

/// Where the document paints in a buffer of `buffer` physical pixels: the frame, in physical
/// pixels. The offset is the band rounded to whole device pixels (blitz paints at an integer
/// offset); the size is what is left, so the band is equally wide on both sides.
pub(crate) fn content_px(buffer: Size, inset: Inset, scale: Scale) -> Rect {
    let offset = logical_to_physical(inset.0, scale);
    Rect {
        x: offset,
        y: offset,
        width: (buffer.width - 2 * offset).max(0),
        height: (buffer.height - 2 * offset).max(0),
    }
}

/// A surface-local pointer position in the document's coordinates: less the painted offset,
/// so a hit test agrees with the pixels to the device pixel.
pub(crate) fn to_document(pos: PointerPos, inset: Inset, scale: Scale) -> PointerPos {
    let offset = f64::from(logical_to_physical(inset.0, scale)) / scale.as_f64();
    PointerPos {
        x: pos.x - offset,
        y: pos.y - offset,
    }
}

/// Document-coordinate rectangles in surface coordinates.
pub(crate) fn to_surface(rects: Vec<Rect>, inset: Inset) -> Vec<Rect> {
    rects
        .into_iter()
        .map(|rect| Rect {
            x: rect.x + inset.0,
            y: rect.y + inset.0,
            ..rect
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: Size = Size {
        width: 414,
        height: 314,
    };
    const BAND: Inset = Inset(7);

    fn at(x: f64, y: f64) -> PointerPos {
        PointerPos { x, y }
    }

    #[test]
    fn the_window_geometry_is_the_frame_and_excludes_the_band() {
        let frame = Size {
            width: 400,
            height: 300,
        };
        let surface = surface_of(frame, BAND);
        assert_eq!(surface, SURFACE);
        assert_eq!(frame_of(surface, BAND), frame);
        assert_eq!(
            window_geometry(surface, BAND),
            Rect {
                x: 7,
                y: 7,
                width: 400,
                height: 300
            }
        );
        assert_eq!(
            window_geometry(frame, Inset::NONE),
            Rect {
                x: 0,
                y: 0,
                width: 400,
                height: 300
            },
            "no band: the whole surface"
        );
    }

    #[test]
    fn the_content_is_inset_by_whole_device_pixels_on_both_sides() {
        let cases = [
            (Scale::ONE, 414, 7, 400),
            (Scale(180), 621, 11, 599),
            (Scale(240), 828, 14, 800),
        ];
        for (scale, buffer_width, offset, width) in cases {
            let buffer = Size {
                width: buffer_width,
                height: buffer_width,
            };
            let content = content_px(buffer, BAND, scale);
            assert_eq!((content.x, content.width), (offset, width), "{scale:?}");
        }
    }

    #[test]
    fn pointer_and_rects_move_between_surface_and_document_coordinates() {
        let doc = to_document(at(17.0, 27.0), BAND, Scale::ONE);
        assert_eq!(doc, at(10.0, 20.0));
        let back = to_surface(
            vec![Rect {
                x: 10,
                y: 20,
                width: 5,
                height: 5,
            }],
            BAND,
        );
        assert_eq!((back[0].x, back[0].y), (17, 27));
    }
}
