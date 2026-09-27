//! The IME request a text field focused before its first layout never made (FINDINGS F71).
//!
//! Blitz asks the shell for the IME in `Node::focus`, and only when the node already has its
//! `TextInputData`; that data is built by layout (`create_text_editor`, first `resolve`). A
//! field focused on mount, as the launcher's is, is focused before its first resolve, so the
//! request is never made: the IME stays off, and every key reaches the field as a raw letter.
//! After each resolve the host therefore asks what Blitz would have: a focused text field
//! whose shell still has the IME off gets it enabled, at the field's content box.

use blitz_dom::BaseDocument;

use crate::dom::shell_provider::ImeRequest;
use crate::region::{Px, Rect};

/// What has the document's focus, as far as the IME is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedField {
    /// A text field (`<input>` of a text type, `<textarea>`), with its content box in
    /// surface-local logical pixels.
    Text { area: Rect },
    /// Anything else, or nothing.
    Other,
}

/// What the shell's IME request needs after a resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    Keep,
    /// Enable the IME at `cursor_area`, as `Node::focus` would have.
    Enable {
        cursor_area: Rect,
    },
}

/// The repair for `request` with `focused` holding the focus. Only a focused text field with the
/// IME off is repaired; an enabled request keeps its own caret, and anything else is the
/// document's own business (an edit surface switches the IME itself). Pure.
pub fn repair(request: ImeRequest, focused: FocusedField) -> Repair {
    match (request, focused) {
        (ImeRequest::Disabled, FocusedField::Text { area }) => Repair::Enable { cursor_area: area },
        (ImeRequest::Enabled { .. }, _) | (_, FocusedField::Other) => Repair::Keep,
    }
}

/// A focused text field's text: `typed` is its value, `shown` adds the live composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldText {
    pub typed: String,
    pub shown: String,
}

/// The focused text field's text, if a text field has the focus.
pub fn focused_text(doc: &BaseDocument) -> Option<FieldText> {
    let node = doc.get_node(doc.get_focussed_node_id()?)?;
    let data = node.element_data()?.text_input_data()?;
    Some(FieldText {
        typed: data.editor.text().to_string(),
        shown: data.editor.raw_text().to_string(),
    })
}

/// The document's focused node, if it is a text field, with its content box.
pub fn focused_field(doc: &BaseDocument) -> FocusedField {
    let Some(node) = doc.get_focussed_node_id().and_then(|id| doc.get_node(id)) else {
        return FocusedField::Other;
    };
    if node
        .element_data()
        .and_then(|element| element.text_input_data())
        .is_none()
    {
        return FocusedField::Other;
    }
    let origin = node.absolute_position(0.0, 0.0);
    let layout = node.final_layout();
    FocusedField::Text {
        area: Rect {
            x: (origin.x + layout.content_box_x()).round() as Px,
            y: (origin.y + layout.content_box_y()).round() as Px,
            width: layout.content_box_width().round() as Px,
            height: layout.content_box_height().round() as Px,
        },
    }
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
            (
                ImeRequest::Disabled,
                FocusedField::Text { area: AREA },
                Repair::Enable { cursor_area: AREA },
            ),
            (
                ImeRequest::Enabled { cursor_area: CARET },
                FocusedField::Text { area: AREA },
                Repair::Keep,
            ),
            (ImeRequest::Disabled, FocusedField::Other, Repair::Keep),
            (
                ImeRequest::Enabled { cursor_area: CARET },
                FocusedField::Other,
                Repair::Keep,
            ),
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
