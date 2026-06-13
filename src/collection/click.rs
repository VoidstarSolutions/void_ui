//! Central selection-click application: maps a row click's modifiers to
//! the matching `SelectionState` operation, keyed by stable row id.

use std::sync::Arc;

use crate::collection::{IdSource, SelectionState, visual_range_ids};

/// Item-data accessor (`Fn(&State) -> &[Item]`).
pub(crate) type ItemsFn<State, Item> = Arc<dyn for<'a> Fn(&'a State) -> &'a [Item] + Send + Sync>;
/// Selection lens (`Fn(&mut State) -> &mut SelectionState`).
pub(crate) type SelectionLens<State> =
    Arc<dyn for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync>;

/// A row click's resolved modifiers (mirrors `data_grid`'s `RowClickAction`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowClick {
    pub(crate) shift: bool,
    pub(crate) action_mod: bool,
}

/// Applies a row click at slice position `pos` to the host's
/// `SelectionState`: shift extends the visual range from the anchor, the
/// action modifier toggles membership, a plain click replaces.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by data_grid migration in the next task")
)]
pub(crate) fn apply_row_click<State, Item>(
    state: &mut State,
    click: RowClick,
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

    if click.shift {
        let anchor = (**sel_lens)(state).anchor();
        let range = anchor.and_then(|anchor_id| {
            let data = (*items)(state);
            visual_range_ids(data, id_source, anchor_id, target_id)
        });
        match range {
            Some(ids) => (**sel_lens)(state).extend_range(ids),
            None => (**sel_lens)(state).replace_with(target_id),
        }
    } else if click.action_mod {
        (**sel_lens)(state).toggle(target_id);
    } else {
        (**sel_lens)(state).replace_with(target_id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ItemsFn, RowClick, SelectionLens, apply_row_click};
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
            RowClick {
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
        let click = RowClick {
            shift: false,
            action_mod: true,
        };
        apply_row_click(&mut s, click, 0, &items, Some(&lens), &id_source);
        apply_row_click(&mut s, click, 0, &items, Some(&lens), &id_source);
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
            RowClick {
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
            RowClick {
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
            RowClick {
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
}
