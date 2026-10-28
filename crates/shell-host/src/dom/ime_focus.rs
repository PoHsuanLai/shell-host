//! The IME request a text field focused before its first layout never made.
//!
//! Blitz asks the shell for the IME in `Node::focus`, and only when the node already has its
//! `TextInputData`; that data is built by layout (`create_text_editor`, first `resolve`). A
//! field focused on mount, as the launcher's is, is focused before its first resolve, so the
//! request is never made: the IME stays off, and every key reaches the field as a raw letter.
//! After each resolve the host therefore asks what Blitz would have: a focused text field
//! whose shell still has the IME off gets it enabled, at the field's content box.

use blitz_dom::{BaseDocument, LocalName};

use crate::region::{Px, Rect};

/// Whether the focused field is secret: a password field's text never goes to the input
/// method, and the field is declared as one (`handlers::text_input`). An
/// `<input type=password>` (any case) is secret.
pub(crate) fn focused_secret(doc: &BaseDocument) -> bool {
    doc.get_focussed_node_id()
        .and_then(|id| doc.get_node(id))
        .filter(|node| {
            node.data
                .is_element_with_tag_name(&LocalName::from("input"))
        })
        .and_then(|node| node.attr(LocalName::from("type")))
        .is_some_and(|kind| kind.eq_ignore_ascii_case("password"))
}

/// The caret rectangle to enable the IME at, as `Node::focus` would have, or `None` for no
/// repair. `request` is the shell's IME request (`None`: off) and `focused` the focused text
/// field's content box. Only a focused text field with the IME off is repaired; an enabled
/// request keeps its own caret, and anything else is the document's own business (an edit
/// surface switches the IME itself). Pure.
pub(crate) fn repair(request: Option<Rect>, focused: Option<Rect>) -> Option<Rect> {
    match request {
        None => focused,
        Some(_) => None,
    }
}

#[cfg(feature = "debug-inject")]
/// A focused text field's text: `typed` is its value, `shown` adds the live composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldText {
    pub typed: String,
    pub shown: String,
}

#[cfg(feature = "debug-inject")]
/// The focused text field's text, if a text field has the focus.
pub(crate) fn focused_text(doc: &BaseDocument) -> Option<FieldText> {
    let node = doc.get_node(doc.get_focussed_node_id()?)?;
    let data = node.element_data()?.text_input_data()?;
    Some(FieldText {
        typed: data.editor.text().to_string(),
        shown: data.editor.raw_text().to_string(),
    })
}

/// The document's focused node, if it is a text field (`<input>` of a text type,
/// `<textarea>`): its content box in surface-local logical pixels.
pub(crate) fn focused_field(doc: &BaseDocument) -> Option<Rect> {
    let node = doc.get_focussed_node_id().and_then(|id| doc.get_node(id))?;
    node.element_data()
        .and_then(|element| element.text_input_data())?;
    let origin = node.absolute_position(0.0, 0.0);
    let layout = node.final_layout();
    Some(Rect {
        x: (origin.x + layout.content_box_x()).round() as Px,
        y: (origin.y + layout.content_box_y()).round() as Px,
        width: layout.content_box_width().round() as Px,
        height: layout.content_box_height().round() as Px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 24,
        y: 26,
        width: 416,
        height: 44,
    };
    const CARET: Rect = Rect {
        x: 30,
        y: 26,
        width: 1,
        height: 44,
    };

    #[test]
    fn only_a_focused_text_field_with_the_ime_off_is_enabled() {
        let cases = [
            (None, Some(AREA), Some(AREA)),
            (Some(CARET), Some(AREA), None),
            (None, None, None),
            (Some(CARET), None, None),
        ];
        for (request, focused, expected) in cases {
            assert_eq!(
                repair(request, focused),
                expected,
                "{request:?} {focused:?}"
            );
        }
    }
}
