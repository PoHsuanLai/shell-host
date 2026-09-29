//! The decisions behind the data-device handler: whose `wl_data_source` an event is for.

/// Whose source an event names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceOwner {
    /// The drag in progress.
    Drag,
    /// The selection this host claimed.
    Selection,
    /// A source the host holds no record of.
    Unknown,
}

/// The owner of `source`, given the identities of the current drag's source and of the
/// selection's source. A drag wins a tie: its source is never also the selection's.
pub(super) fn source_owner<T: PartialEq>(
    source: &T,
    drag: Option<&T>,
    selection: Option<&T>,
) -> SourceOwner {
    match (drag, selection) {
        (Some(drag), _) if drag == source => SourceOwner::Drag,
        (_, Some(selection)) if selection == source => SourceOwner::Selection,
        _ => SourceOwner::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Case = (&'static str, u32, Option<u32>, Option<u32>, SourceOwner);

    #[test]
    fn a_source_belongs_to_the_drag_the_selection_or_nobody() {
        let cases: &[Case] = &[
            ("the drag's", 1, Some(1), Some(2), SourceOwner::Drag),
            (
                "the selection's",
                2,
                Some(1),
                Some(2),
                SourceOwner::Selection,
            ),
            (
                "neither of the two held",
                3,
                Some(1),
                Some(2),
                SourceOwner::Unknown,
            ),
            (
                "no drag, the selection's",
                2,
                None,
                Some(2),
                SourceOwner::Selection,
            ),
            (
                "no drag, not the selection's",
                3,
                None,
                Some(2),
                SourceOwner::Unknown,
            ),
            ("nothing held at all", 1, None, None, SourceOwner::Unknown),
            ("drag wins a tie", 1, Some(1), Some(1), SourceOwner::Drag),
        ];
        for (name, source, drag, selection, want) in cases {
            assert_eq!(
                source_owner(source, drag.as_ref(), selection.as_ref()),
                *want,
                "{name}"
            );
        }
    }
}
