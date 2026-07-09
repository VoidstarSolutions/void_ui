//! Central selection-click application: maps a row click's modifiers to
//! the matching `SelectionState` operation, keyed by stable row id.

use std::sync::Arc;

use super::row_click::RowClickAction;
use crate::collection::{IdSource, SelectionState, visual_range_ids};

/// Item-data accessor (`Fn(&State) -> &[Item]`).
pub(crate) type ItemsFn<State, Item> = Arc<dyn for<'a> Fn(&'a State) -> &'a [Item] + Send + Sync>;
/// Selection lens (`Fn(&mut State) -> &mut SelectionState`).
pub(crate) type SelectionLens<State> =
    Arc<dyn for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync>;
/// Host row-activation callback: `(state, row_id)`, run when Enter is pressed
/// on the focused row. The substrate resolves the row's stable id and hands
/// it in; see [`ClickableRow::on_activate`](super::row_click::ClickableRow).
pub(crate) type OnActivate<State> = Arc<dyn Fn(&mut State, u64) + Send + Sync>;

/// Applies a row click at slice position `pos` to the host's
/// `SelectionState`: shift extends the visual range from the anchor, the
/// action modifier toggles membership, a plain click replaces.
pub(crate) fn apply_row_click<State, Item>(
    state: &mut State,
    action: RowClickAction,
    pos: usize,
    items: &ItemsFn<State, Item>,
    selection_lens: Option<&SelectionLens<State>>,
    id_source: &IdSource<Item>,
) {
    let Some(sel_lens) = selection_lens else {
        return;
    };
    let Some(target_id) = ({
        let data = (*items)(state);
        data.get(pos).map(|item| id_source.id_of(pos, item))
    }) else {
        return;
    };

    if action.shift {
        let anchor = (**sel_lens)(state).anchor();
        let range = anchor.and_then(|anchor_id| {
            let data = (*items)(state);
            visual_range_ids(data, id_source, anchor_id, target_id)
        });
        match range {
            Some(ids) => (**sel_lens)(state).extend_range(ids),
            None => (**sel_lens)(state).replace_with(target_id),
        }
    } else if action.action_mod {
        (**sel_lens)(state).toggle(target_id);
    } else {
        (**sel_lens)(state).replace_with(target_id);
    }
}

/// Resolves the stable row id at slice position `pos` (the same lookup
/// [`apply_row_click`] uses, so activation and selection agree on identity
/// under host-side reordering) and hands it to the host's activation
/// callback. A `pos` past the end of the data — a stale row after the host
/// shrank the collection — resolves to nothing and is a no-op.
pub(crate) fn apply_row_activate<State, Item>(
    state: &mut State,
    pos: usize,
    items: &ItemsFn<State, Item>,
    id_source: &IdSource<Item>,
    on_activate: &OnActivate<State>,
) {
    let Some(target_id) = ({
        let data = (*items)(state);
        data.get(pos).map(|item| id_source.id_of(pos, item))
    }) else {
        return;
    };
    on_activate(state, target_id);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ItemsFn, OnActivate, SelectionLens, apply_row_activate, apply_row_click};
    use crate::collection::row_click::RowClickAction;
    use crate::collection::{IdSource, SelectionState};

    struct S {
        items: Vec<u64>,
        sel: SelectionState,
    }

    fn fixtures() -> (ItemsFn<S, u64>, SelectionLens<S>, IdSource<u64>) {
        let items: ItemsFn<S, u64> = Arc::new(|s: &S| &s.items[..]);
        let lens: SelectionLens<S> = Arc::new(|s: &mut S| &mut s.sel);
        let id_source = IdSource::Explicit(Arc::new(|item: &u64| *item));
        (items, lens, id_source)
    }

    #[test]
    fn plain_click_replaces_selection_with_target_id() {
        let mut s = S {
            items: vec![10, 20, 30],
            sel: SelectionState::new(),
        };
        let (items, lens, id_source) = fixtures();
        apply_row_click(
            &mut s,
            RowClickAction {
                shift: false,
                action_mod: false,
            },
            1,
            &items,
            Some(&lens),
            &id_source,
        );
        assert!(s.sel.contains(20));
        assert_eq!(s.sel.len(), 1);
        assert_eq!(s.sel.anchor(), Some(20));
    }

    #[test]
    fn action_mod_toggles_membership() {
        let mut s = S {
            items: vec![10, 20, 30],
            sel: SelectionState::new(),
        };
        let (items, lens, id_source) = fixtures();
        let action = RowClickAction {
            shift: false,
            action_mod: true,
        };
        apply_row_click(&mut s, action, 0, &items, Some(&lens), &id_source);
        apply_row_click(&mut s, action, 0, &items, Some(&lens), &id_source);
        assert!(!s.sel.contains(10));
    }

    #[test]
    fn shift_extends_visual_range_from_anchor() {
        let mut s = S {
            items: vec![10, 20, 30, 40],
            sel: SelectionState::new(),
        };
        let (items, lens, id_source) = fixtures();
        apply_row_click(
            &mut s,
            RowClickAction {
                shift: false,
                action_mod: false,
            },
            0,
            &items,
            Some(&lens),
            &id_source,
        );
        apply_row_click(
            &mut s,
            RowClickAction {
                shift: true,
                action_mod: false,
            },
            2,
            &items,
            Some(&lens),
            &id_source,
        );
        assert_eq!(s.sel.iter().collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(s.sel.anchor(), Some(10));
    }

    #[test]
    fn no_lens_is_a_noop() {
        let mut s = S {
            items: vec![10],
            sel: SelectionState::new(),
        };
        let (items, _lens, id_source) = fixtures();
        apply_row_click(
            &mut s,
            RowClickAction {
                shift: false,
                action_mod: false,
            },
            0,
            &items,
            None,
            &id_source,
        );
        assert!(s.sel.is_empty());
    }

    /// Host state for the activation tests: records the id the callback was
    /// handed (or `None` if never invoked).
    struct A {
        items: Vec<u64>,
        got: Option<u64>,
    }

    fn recording_activate() -> OnActivate<A> {
        Arc::new(|s: &mut A, id: u64| s.got = Some(id))
    }

    /// The substrate resolves the row's *stable id* (not its slice position)
    /// and hands it to the host: activating slice pos 1 of `[10, 20, 30]`
    /// yields id `20`. This is the promise that survives host-side
    /// sort/filter reordering — the id follows the item, not the row index.
    #[test]
    fn activate_hands_the_stable_row_id_to_the_host() {
        let items: ItemsFn<A, u64> = Arc::new(|s: &A| &s.items[..]);
        let id_source = IdSource::Explicit(Arc::new(|item: &u64| *item));
        let on_activate = recording_activate();

        let mut s = A {
            items: vec![10, 20, 30],
            got: None,
        };
        apply_row_activate(&mut s, 1, &items, &id_source, &on_activate);
        assert_eq!(s.got, Some(20), "the item's stable id, not the position");
    }

    /// With no id projector, the position fallback hands the slice position
    /// itself as the id.
    #[test]
    fn activate_with_position_id_source_hands_the_slice_position() {
        let items: ItemsFn<A, u64> = Arc::new(|s: &A| &s.items[..]);
        let on_activate = recording_activate();

        let mut s = A {
            items: vec![10, 20, 30],
            got: None,
        };
        apply_row_activate(&mut s, 2, &items, &IdSource::Position, &on_activate);
        assert_eq!(s.got, Some(2), "position fallback id");
    }

    /// A stale row position past the end of the data (the host shrank the
    /// collection out from under a pending activation) resolves to nothing:
    /// the host callback is never invoked.
    #[test]
    fn activate_past_the_end_never_calls_the_host() {
        let items: ItemsFn<A, u64> = Arc::new(|s: &A| &s.items[..]);
        let id_source = IdSource::Explicit(Arc::new(|item: &u64| *item));
        let on_activate = recording_activate();

        let mut s = A {
            items: vec![10, 20],
            got: None,
        };
        apply_row_activate(&mut s, 5, &items, &id_source, &on_activate);
        assert_eq!(s.got, None, "out-of-range pos is a no-op");
    }
}
