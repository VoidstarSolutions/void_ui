//! Xilem [`View`] composition for the data grid.
//!
//! - The body is `flex_col(header, collection_body(..))`: the grid
//!   supplies only per-row *content* (one `ColumnStrip` of fixed-width
//!   cells per loaded index) via `render_row`, and the collection
//!   substrate's `collection_body` owns
//!   virtualization, scroll-to-row, arrow-key row navigation, the
//!   selection background, and the modifier-aware click routing (shift /
//!   ctrl-cmd) that updates the [`SelectionState`].
//! - The header + filter + body stack is wrapped in a horizontal-only
//!   [`scroll_container`](crate::components::scroll_container) so columns
//!   wider than the viewport are reachable (header and body share the
//!   horizontal offset because they're one child subtree), then in
//!   [`CopyOnShortcutView`], a private wrapper for the
//!   [`CopyOnShortcut`](super::CopyOnShortcut) masonry widget that
//!   pushes a fresh TSV projection of the current selection on every
//!   rebuild and dumps it to the platform clipboard on Ctrl/Cmd+C.
//! - Selected rows are styled with the theme's `surface_2` panel
//!   color.

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use xilem::WidgetView;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{
    AnyFlexChild, CrossAxisAlignment, FlexSpacer, MainAxisAlignment, flex_col, flex_item, flex_row,
    label, sized_box, text_input,
};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use super::column::{CellAlign, CellRenderer, ColumnDef, ColumnId, TextProjector};
use super::column_strip::{SeparatorStyle, column_strip};
use super::copy_shortcut::{CopyOnShortcut, CopyRequested};
use super::expand::{disclosure_hit_width, disclosure_toggle, tree_indent_step};
use super::filter::FilterState;
use super::header_click::clickable_header;
use super::sort::{SortDirection, SortState};
use super::width::ColumnWidths;
use crate::Theme;
use crate::collection::ScrollState;
use crate::collection::SelectionState;
use crate::collection::{
    CollectionBodyParams, IdSource, ItemsFn, Lazy, LeadingHitZone, LeadingHitZoneFn, RenderRow,
    SelectionLens, collection_body,
};
use crate::components::icon::disclosure_chevron;
use crate::components::scroll_container::scroll_container;
use crate::theme::Density;

/// Boxed row-data accessor, shared via `Arc` across the body and clipboard
/// closures. Aliases the substrate's [`ItemsFn`] — same `Fn(&State) -> &[R]`
/// shape — so the grid and the collection body name one type, not two.
type RowsFn<State, R> = ItemsFn<State, R>;
/// Boxed stable row-id projector (`Fn(&R) -> u64`). Supplied by the host
/// (the `getRowId` contract); selection is keyed by this id rather than
/// by slice position, so it follows rows across host-side sort/filter.
type RowIdFn<R> = Arc<dyn Fn(&R) -> u64 + Send + Sync>;
/// Boxed sort-change callback (`Fn(&mut State, ColumnId, multi)`). A
/// header click emits the clicked column's stable id + whether Shift was
/// held (`multi`); the host cycles its [`SortState`] — replacing for a
/// plain click, adding/cycling a tiebreaker for a multi (Shift) click —
/// *and* re-derives its ordered view. The grid never reorders data itself
/// (the same host-side shape as [`FilterChange`]). Keyed by id so an
/// active sort stays attached across reorder/hide.
type SortChange<State, Action> = Arc<dyn Fn(&mut State, ColumnId, bool) -> Action + Send + Sync>;

/// One-shot warning that the grid is configured with selection + a
/// reorder source but no stable `row_id`. Emitted at most once per
/// process (an `AtomicBool` latch) so it surfaces the misconfiguration
/// without spamming on every rebuild.
fn warn_missing_row_id() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "data_grid: selection is enabled with sorting/filtering but no \
             `.row_id(...)` was supplied — selection is keyed by slice \
             position and will point at the wrong rows after a reorder. \
             Supply a stable, unique row id (the `getRowId` contract)."
        );
    }
}

/// Emits the stable-`row_id` footgun warnings (each at most once) for a
/// grid that reorders or re-flattens rows without a stable id. Both cases
/// key state (selection / expansion) by row id, so the slice-position
/// fallback points at the wrong row after the host reorders.
///
/// - selection + a reorder source (sort/filter) but no id → wrong row
///   selected after a reorder.
/// - expansion but no id → wrong row toggled after a re-flatten.
fn warn_id_footguns(has_row_id: bool, selection_reorders: bool, has_expandable: bool) {
    // A stable id makes both keying schemes correct across reorders, so
    // there's nothing to warn about.
    if has_row_id {
        return;
    }
    if selection_reorders {
        warn_missing_row_id();
    }
    if has_expandable {
        // Unlike selection (which degrades to positional keying — correct for
        // a static grid), expansion is simply *broken* without a stable id:
        // the tree cell can't know a row's position, so every chevron toggles
        // the same entry. Fail loudly in debug so misuse can't ship silently;
        // asserting on `has_row_id` (a runtime value that is `false` here)
        // rather than a literal avoids clippy's constant-assertion lint.
        debug_assert!(
            has_row_id,
            "data_grid: `.expandable(...)` requires `.row_id(...)`; without it \
             every chevron toggles the same expansion entry (id 0)."
        );
        warn_expandable_without_row_id();
    }
}

/// One-shot (release-build) warning that [`DataGrid::expandable`] was
/// configured without a stable [`row_id`](DataGrid::row_id). The tree cell
/// resolves each row's toggle id from `&R` alone — it has no slice position
/// — so under the position fallback the id is always `0` and **every chevron
/// toggles the same expansion entry**: expansion is non-functional, not
/// merely wrong after a re-flatten. Debug builds hard-fail (see
/// [`warn_id_footguns`]); release builds warn once. Emitted at most once.
fn warn_expandable_without_row_id() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "data_grid: `.expandable(...)` is set without `.row_id(...)` — the \
             tree cell can't resolve a row's position, so every chevron toggles \
             the same expansion entry (id 0). Expansion is non-functional until \
             you supply a stable, unique row id."
        );
    }
}

/// One-shot warning that two columns resolved to the same [`ColumnId`]
/// (commonly: two columns with the same title and no explicit
/// [`ColumnDef::id`](super::column::ColumnDef::id)). The later column is
/// silently shadowed — sort/filter/width resolution always hits the
/// first. A debug build hard-asserts this; release builds get this warn
/// (emitted once per process) instead of failing silently.
fn warn_duplicate_column_id() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "data_grid: two columns share a ColumnId (same title with no \
             explicit `.id(...)`?) — their sort/filter/width state collides \
             and the later column is shadowed. Give columns distinct ids."
        );
    }
}

/// Boxed filter-change callback (`Fn(&mut State, ColumnId, query)`). The
/// grid emits filter edits through this so the host can update its
/// [`FilterState`] *and* recompute its filtered view — the grid never
/// applies the filter to data itself. Keyed by id (the view translates
/// the filter-row's positional index to a [`ColumnId`]) so a query stays
/// attached to its column across reorder/hide.
type FilterChange<State, Action> =
    Arc<dyn Fn(&mut State, ColumnId, String) -> Action + Send + Sync>;
/// Boxed column-resize callback (`Fn(&mut State, ColumnId, new_width)`).
/// A header resize handle emits the resized column's stable id + proposed
/// absolute width through this so the host can update its
/// [`ColumnWidths`](super::width::ColumnWidths) (which clamps). The view
/// translates the strip's positional resize index to the column's id, so
/// the override stays attached across reorder/hide.
type WidthChange<State, Action> = Arc<dyn Fn(&mut State, ColumnId, f64) -> Action + Send + Sync>;

/// Boxed per-row "is this row expanded?" predicate — the host answers from
/// its [`ExpansionState`](super::expand::ExpansionState); drives the chevron
/// direction.
type IsExpandedFn<R> = Arc<dyn Fn(&R) -> bool + Send + Sync>;
/// Boxed per-row "does this row have children?" predicate — decides whether
/// the tree cell shows a chevron or an inert (alignment) spacer.
type HasChildrenFn<R> = Arc<dyn Fn(&R) -> bool + Send + Sync>;
/// Boxed per-row depth accessor (`0` = root); drives the tree-column indent.
type DepthFn<R> = Arc<dyn Fn(&R) -> u16 + Send + Sync>;
/// Boxed expand/collapse callback (`Fn(&mut State, row_id)`). A chevron
/// click invokes it with the row's stable id so the host flips its
/// [`ExpansionState`](super::expand::ExpansionState) and re-derives its
/// flattened slice. This mirrors selection's per-row click callback — a
/// body-row interaction mutates host state and yields `Action::default()`,
/// unlike the Action-returning *header* callbacks (sort/filter/resize) —
/// because the collection body is `()`-typed (a control in row content
/// can't return a propagating host `Action`).
type ToggleFn<State> = Arc<dyn Fn(&mut State, u64) + Send + Sync>;

/// The four host-supplied closures that turn the grid's **first column**
/// into a tree column with expand/collapse chevrons. Stored together on the
/// builder; see [`DataGrid::expandable`].
struct ExpandableConfig<State, R> {
    is_expanded: IsExpandedFn<R>,
    has_children: HasChildrenFn<R>,
    depth: DepthFn<R>,
    on_toggle: ToggleFn<State>,
}

// Hand-written (not derived) so cloning doesn't demand `State: Clone` /
// `R: Clone` — every field is an `Arc`, so a clone is just a refcount bump.
impl<State, R> Clone for ExpandableConfig<State, R> {
    fn clone(&self) -> Self {
        Self {
            is_expanded: Arc::clone(&self.is_expanded),
            has_children: Arc::clone(&self.has_children),
            depth: Arc::clone(&self.depth),
            on_toggle: Arc::clone(&self.on_toggle),
        }
    }
}

/// One column's rendering + layout slot — the half of [`ColumnDef`]
/// that's needed at row-build time. Shared (via `Arc`) between the
/// header builder and the row builder closures.
struct ColumnRender<R, State> {
    /// Stable column identity (explicit or title-derived). Column state
    /// — sort, filter, width — is keyed by this, never by slice position,
    /// so the view translates a strip's positional index to this id at
    /// the host-callback boundary.
    id: ColumnId,
    title: String,
    width: f64,
    align: CellAlign,
    render: CellRenderer<R, State>,
}

/// Translates [`CellAlign`] into the [`MainAxisAlignment`] used by the
/// single-child flex wrapper that aligns the cell view inside its
/// fixed-width slot.
const fn align_to_main(align: CellAlign) -> MainAxisAlignment {
    match align {
        CellAlign::Start => MainAxisAlignment::Start,
        CellAlign::Center => MainAxisAlignment::Center,
        CellAlign::End => MainAxisAlignment::End,
    }
}

/// Builder for a virtualized, theme-driven data grid view.
///
/// Construct with [`DataGrid::new`], attach data and behavior through
/// the chained setters, then materialize the xilem view with
/// [`DataGrid::render`]. Lenses are stored boxed, so each future
/// feature is a new method rather than another positional parameter.
///
/// ```ignore
/// DataGrid::new(columns)
///     .rows(|s: &State| &s.ticks[..])
///     .row_count(n)
///     .selection(|s| &mut s.selection)
///     .sort(sort_snapshot, |s| &mut s.sort)
///     .row_height(22.0)
///     .render(&theme)
/// ```
///
/// `selection` and `sort` are optional — omit them for a
/// non-selectable / unsorted grid.
///
/// # Notes
///
/// - Columns wider than the viewport are reachable via the grid's
///   horizontal scroll (the header/filter/body stack scrolls together).
/// - The clipboard (TSV) payload is recomputed on each rebuild from the
///   current selection, in display order; columns without a `text`
///   projector contribute empty cells so spreadsheet paste keeps the
///   column layout.
#[must_use = "DataGrid does nothing until rendered with .render(&theme)"]
pub struct DataGrid<State, R, Action = ()> {
    columns: Vec<ColumnDef<R, State>>,
    row_count: u64,
    row_height: Option<f64>,
    rows: Option<RowsFn<State, R>>,
    selection_lens: Option<SelectionLens<State>>,
    row_id: Option<RowIdFn<R>>,
    sort: SortState,
    sort_change: Option<SortChange<State, Action>>,
    filter: FilterState,
    filter_change: Option<FilterChange<State, Action>>,
    column_widths: ColumnWidths,
    width_change: Option<WidthChange<State, Action>>,
    scroll: ScrollState,
    expandable: Option<ExpandableConfig<State, R>>,
}

/// Default row height when [`DataGrid::row_height`] is unset: the theme's
/// density row baseline (24 px at balanced — the pre-token default).
fn default_row_height(density: &Density) -> f64 {
    f64::from(density.row_height)
}

impl<State, R, Action> DataGrid<State, R, Action>
where
    State: 'static,
    R: 'static,
    Action: Default + 'static,
{
    /// Starts a grid from its column descriptors. Attach data with
    /// [`Self::rows`] + [`Self::row_count`] before rendering.
    pub fn new(columns: Vec<ColumnDef<R, State>>) -> Self {
        Self {
            columns,
            row_count: 0,
            row_height: None,
            rows: None,
            selection_lens: None,
            row_id: None,
            sort: SortState::new(),
            sort_change: None,
            filter: FilterState::new(),
            filter_change: None,
            column_widths: ColumnWidths::new(),
            width_change: None,
            scroll: ScrollState::new(),
            expandable: None,
        }
    }

    /// Sets the row-data accessor. Required for the grid to show data;
    /// without it the body renders empty.
    pub fn rows<F>(mut self, rows: F) -> Self
    where
        F: for<'a> Fn(&'a State) -> &'a [R] + Send + Sync + 'static,
    {
        let rows: RowsFn<State, R> = Arc::new(rows);
        self.rows = Some(rows);
        self
    }

    /// Sets the current row count — the body virtualizes over
    /// `0..row_count`. Snapshot it from host state at frame time.
    pub fn row_count(mut self, row_count: u64) -> Self {
        self.row_count = row_count;
        self
    }

    /// Fixed pixel row height (defaults to the theme's density row baseline).
    pub fn row_height(mut self, row_height: f64) -> Self {
        self.row_height = Some(row_height);
        self
    }

    /// Enables row selection via a lens into the host's
    /// [`SelectionState`]. Selection is keyed by the **stable row id**
    /// from [`Self::row_id`], so it follows its rows across host-side
    /// sort/filter reordering. Omit for a non-selectable grid.
    ///
    /// Pair this with [`Self::row_id`]: without a row-id projector the
    /// grid falls back to using each row's slice position as its id,
    /// which is only correct for a static, unsorted, unfiltered grid.
    pub fn selection<F>(mut self, lens: F) -> Self
    where
        F: for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync + 'static,
    {
        let lens: SelectionLens<State> = Arc::new(lens);
        self.selection_lens = Some(lens);
        self
    }

    /// Supplies a **stable, unique row id** for each row — the grid's
    /// `getRowId` (the same contract as `TanStack` Table / AG Grid / Kendo).
    ///
    /// Selection is keyed by this id, so it stays attached to the right
    /// rows when the host reorders the slice via sorting or filtering.
    /// The id must be **stable** (the same row always yields the same id)
    /// and **unique** across the dataset; a database key or a monotonic
    /// sequence assigned at row creation is ideal.
    ///
    /// If omitted, the grid uses each row's current slice position as its
    /// id — fine for a static grid, but under sorting/filtering a
    /// positional key makes the selection point at whatever row now
    /// occupies that slot (the documented index-keying failure mode). The
    /// grid emits a one-shot `tracing::warn!` if selection and a reorder
    /// source (sort/filter) are wired without a `row_id`.
    ///
    /// Uniqueness matters: if two rows project the same id, they share a
    /// selection membership — selecting or copying one acts on both. A
    /// debug build asserts uniqueness across the *selected* rows during
    /// clipboard copy; in release the contract is the caller's to keep.
    pub fn row_id<F>(mut self, id: F) -> Self
    where
        F: Fn(&R) -> u64 + Send + Sync + 'static,
    {
        self.row_id = Some(Arc::new(id));
        self
    }

    /// Enables sorting. `state` is the current [`SortState`] snapshot
    /// (drives the header arrows + priority badges), and `on_sort` is
    /// invoked as `(state, column, multi)` when the user clicks a sortable
    /// column's header — `multi` is `true` when Shift was held.
    ///
    /// Per the grid's host-owns-order model (the same shape as
    /// [`Self::filter`]), `on_sort` must cycle the host's [`SortState`]
    /// — [`SortState::cycle`](super::sort::SortState::cycle) for a plain
    /// click (replace), [`cycle_additive`](super::sort::SortState::cycle_additive)
    /// for a multi (Shift) click (add/cycle a tiebreaker) — *and* re-derive
    /// whatever ordered view the grid's `rows` accessor serves, typically
    /// by composing [`filtered_indices`](super::filter::filtered_indices)
    /// then [`sort_indices`](super::sort::sort_indices) over the host's
    /// data. The grid never reorders data itself. A column is only
    /// sortable if its [`ColumnDef`] carries a comparator (see
    /// [`ColumnDef::sortable_by_key`]). Omit for an unsorted grid.
    pub fn sort<F>(mut self, state: SortState, on_sort: F) -> Self
    where
        F: Fn(&mut State, ColumnId, bool) -> Action + Send + Sync + 'static,
    {
        self.sort = state;
        self.sort_change = Some(Arc::new(on_sort));
        self
    }

    /// Enables filtering. `filter` is the current [`FilterState`]
    /// snapshot (drives the per-column filter inputs and the persistent
    /// "filtered" indicator), and `on_change` is invoked as
    /// `(state, column, query)` whenever the user edits a column's
    /// filter input.
    ///
    /// Per the grid's host-filters model, `on_change` must update the
    /// host's `FilterState` *and* recompute whatever filtered view the
    /// grid's `rows` accessor serves — the grid never touches the data
    /// itself. A filter input is shown only for columns whose
    /// [`ColumnDef`] carries a predicate (see
    /// [`ColumnDef::filterable_by_text`]); filtered columns also get an
    /// always-visible accent + marker so a filtered view is never
    /// mistaken for the full data set.
    pub fn filter<F>(mut self, filter: FilterState, on_change: F) -> Self
    where
        F: Fn(&mut State, ColumnId, String) -> Action + Send + Sync + 'static,
    {
        let on_change: FilterChange<State, Action> = Arc::new(on_change);
        self.filter = filter;
        self.filter_change = Some(on_change);
        self
    }

    /// Supplies the current [`ColumnWidths`] snapshot. Columns with an
    /// override render at that width; the rest use their [`ColumnDef`]
    /// default. Drives both the per-column layout and the total content
    /// width that the horizontal scroll extent is based on. Defaults to
    /// empty (every column at its default width).
    pub fn column_widths(mut self, widths: ColumnWidths) -> Self {
        self.column_widths = widths;
        self
    }

    /// Enables drag-to-resize columns. Each header cell gains a
    /// trailing-edge handle; dragging it calls
    /// `on_resize(state, column_id, new_width)` with the resized column's
    /// stable [`ColumnId`] and proposed absolute width. The host applies
    /// it to its [`ColumnWidths`] (which clamps to
    /// [`MIN_COLUMN_WIDTH`](super::width::MIN_COLUMN_WIDTH)) and passes the
    /// updated snapshot back via [`Self::column_widths`]. Keying by id
    /// means a width override stays attached across reorder/hide. Omit to
    /// leave columns non-resizable.
    pub fn on_column_resize<F>(mut self, on_resize: F) -> Self
    where
        F: Fn(&mut State, ColumnId, f64) -> Action + Send + Sync + 'static,
    {
        self.width_change = Some(Arc::new(on_resize));
        self
    }

    /// Supplies the current [`ScrollState`] snapshot. When the
    /// snapshot's generation differs from the one the grid last
    /// applied, the body scrolls so the requested row's top aligns with
    /// the top of the viewport (masonry's `overwrite_anchor`
    /// semantics). The index is a display position in the host's
    /// ordered view — the same domain as [`Self::row_count`] — and is
    /// clamped to the row range; a request against an empty grid is a
    /// no-op. Defaults to no request.
    ///
    /// The host keeps the `ScrollState` in its app state and calls
    /// [`ScrollState::scroll_to_index`] from any callback:
    ///
    /// ```ignore
    /// // In app state: scroll: ScrollState,
    /// // In any callback:
    /// state.scroll.scroll_to_index(50_000);
    /// // At frame time:
    /// data_grid(columns).scroll_to(state.scroll) /* ... */
    /// ```
    pub fn scroll_to(mut self, scroll: ScrollState) -> Self {
        self.scroll = scroll;
        self
    }

    /// Turns the grid's **first column** into a tree column with
    /// expand/collapse chevrons, for master-detail / accordion / tree-table
    /// layouts.
    ///
    /// Per the grid's host-owns-the-data model (the same shape as sorting
    /// and filtering), the grid does **not** grow or hide child rows. The
    /// host keeps an [`ExpansionState`](super::expand::ExpansionState) and,
    /// when it changes, splices the now-visible descendants into the flat
    /// `&[R]` slice it serves and bumps [`row_count`](Self::row_count).
    /// Children become ordinary flat rows, so they inherit virtualization,
    /// the fixed row height, scroll, and selection for free.
    ///
    /// The four closures answer per-row questions and report the toggle:
    /// - `is_expanded` — is this row currently expanded? (chevron points
    ///   down when `true`, right when `false`)
    /// - `has_children` — does it have children? (a chevron is shown only
    ///   when `true`; leaf rows get an aligned spacer instead)
    /// - `depth` — its depth (`0` = root), which sets the tree-column indent
    /// - `on_toggle` — invoked as `(state, row_id)` when the chevron is
    ///   clicked; flip the host's `ExpansionState` for that id and re-derive
    ///   the flattened slice. Like selection, this mutates host state and
    ///   re-runs the app logic; it does not return a host `Action`.
    ///
    /// The toggle keys by the **stable row id** from [`Self::row_id`], so an
    /// expanded row stays expanded across host-side sort/filter reordering —
    /// pair `.expandable(..)` with `.row_id(..)` (a one-shot warning fires
    /// otherwise, the same footgun guard selection uses).
    ///
    /// The chevron renders in the first column's cell (indented by depth),
    /// so make that column wide enough to hold the indent + chevron + its
    /// own content. A click on the chevron toggles rather than selects (the
    /// row defers that leading zone to the chevron control); a click
    /// anywhere else on the row still selects as usual.
    pub fn expandable<IsExp, Kids, D, Tog>(
        mut self,
        is_expanded: IsExp,
        has_children: Kids,
        depth: D,
        on_toggle: Tog,
    ) -> Self
    where
        IsExp: Fn(&R) -> bool + Send + Sync + 'static,
        Kids: Fn(&R) -> bool + Send + Sync + 'static,
        D: Fn(&R) -> u16 + Send + Sync + 'static,
        Tog: Fn(&mut State, u64) + Send + Sync + 'static,
    {
        self.expandable = Some(ExpandableConfig {
            is_expanded: Arc::new(is_expanded),
            has_children: Arc::new(has_children),
            depth: Arc::new(depth),
            on_toggle: Arc::new(on_toggle),
        });
        self
    }

    /// Materializes the xilem view at the supplied theme.
    #[must_use]
    pub fn render(self, theme: &Theme) -> impl WidgetView<State, Action> + use<State, R, Action> {
        build_grid_view(self, theme)
    }
}

/// Starts a [`DataGrid`] from its column descriptors — the free-function
/// entry point mirroring the other components' constructors
/// (`button(..)`, `checkbox(..)`, …). Equivalent to [`DataGrid::new`];
/// attach data/behavior with the chained setters, then [`DataGrid::render`].
///
/// ```ignore
/// data_grid(columns)
///     .rows(|s: &State| &s.rows[..])
///     .row_count(n)
///     .render(&theme)
/// ```
pub fn data_grid<State, R, Action>(columns: Vec<ColumnDef<R, State>>) -> DataGrid<State, R, Action>
where
    State: 'static,
    R: 'static,
    Action: Default + 'static,
{
    DataGrid::new(columns)
}

/// The per-column data the view layer needs, decomposed from a
/// `Vec<ColumnDef>` into parallel, index-aligned collections. A named
/// struct (rather than a tuple) so callers read fields by name and
/// reordering can't silently swap two same-typed members.
///
/// The `Arc`-wrapped members are shared between the synchronous header
/// builder and the `virtual_scroll` row-builder closure.
struct DecomposedColumns<R, State> {
    /// Per-column rendering slot (title, effective width, align, renderer).
    render_slots: Arc<Vec<ColumnRender<R, State>>>,
    /// Per-column clipboard text projector (`None` ⇒ empty TSV cell).
    text_projectors: Arc<Vec<Option<TextProjector<R>>>>,
    /// Per-column "is sortable" flag (the column carries a comparator).
    /// Only the *flag* survives: the host owns sorting now, so the grid
    /// keeps the comparator only long enough to decide whether the header
    /// is clickable — it never reorders data itself.
    sortable: Vec<bool>,
    /// Per-column "is filterable" flag (a predicate is present). The
    /// predicate itself is dropped — the host applies filtering — so the
    /// grid keeps only the flag (to decide whether to show a filter input).
    filterable: Vec<bool>,
}

/// Splits each [`ColumnDef`] into the parallel collections of
/// [`DecomposedColumns`]. Each slot's `width` is the *effective* width
/// (override from `widths`, else the column default), so every width
/// consumer reads it uniformly.
fn decompose_columns<R, State>(
    columns: Vec<ColumnDef<R, State>>,
    widths: &ColumnWidths,
) -> DecomposedColumns<R, State> {
    let mut render_slots: Vec<ColumnRender<R, State>> = Vec::with_capacity(columns.len());
    let mut text_projectors: Vec<Option<TextProjector<R>>> = Vec::with_capacity(columns.len());
    let mut sortable: Vec<bool> = Vec::with_capacity(columns.len());
    let mut filterable: Vec<bool> = Vec::with_capacity(columns.len());
    // The `ColumnId` contract requires uniqueness, since column state
    // (sort/filter/width) is keyed by it; a duplicate makes two columns
    // share state and the later one is silently shadowed (`.find()` in
    // sort/filter resolution always hits the first). Checking is cheap
    // (once per rebuild over a handful of columns), so unlike the per-row
    // `row_id` case we can afford a real release-build signal: a one-shot
    // `tracing::warn!` *plus* a debug assert for a hard fail in tests.
    let mut seen_ids = std::collections::BTreeSet::<ColumnId>::new();
    for col in columns {
        let id = col.effective_id();
        let first_seen = seen_ids.insert(id.clone());
        debug_assert!(
            first_seen,
            "data_grid: column id {id} is not unique — two columns share \
             an id, so their sort/filter/width state would collide (set a \
             distinct ColumnDef::id; see ColumnId)"
        );
        if !first_seen {
            warn_duplicate_column_id();
        }
        text_projectors.push(col.text);
        // Comparator presence ⇒ sortable; the comparator itself is the
        // host's (via `sort_indices`), so we drop it here.
        sortable.push(col.comparator.is_some());
        filterable.push(col.filter.is_some());
        render_slots.push(ColumnRender {
            width: widths.effective(&id, col.width),
            title: col.title,
            align: col.align,
            render: col.render,
            id,
        });
    }
    DecomposedColumns {
        render_slots: Arc::new(render_slots),
        text_projectors: Arc::new(text_projectors),
        sortable,
        filterable,
    }
}

/// Turns a finished [`DataGrid`] builder into the view tree. Kept as a
/// free function so the body stays flat and `render` is a thin call.
fn build_grid_view<State, R, Action>(
    grid: DataGrid<State, R, Action>,
    theme: &Theme,
) -> impl WidgetView<State, Action> + use<State, R, Action>
where
    State: 'static,
    R: 'static,
    Action: Default + 'static,
{
    let theme = *theme;
    let DataGrid {
        columns,
        row_count,
        row_height,
        rows,
        selection_lens,
        row_id,
        sort,
        sort_change,
        filter,
        filter_change,
        column_widths,
        width_change,
        scroll,
        expandable,
    } = grid;
    let row_height = row_height.unwrap_or_else(|| default_row_height(&theme.density));

    // Default the data accessor to an empty slice when unset.
    let rows: RowsFn<State, R> = rows.unwrap_or_else(|| {
        let empty: RowsFn<State, R> = Arc::new(|_: &State| -> &[R] { &[] });
        empty
    });

    warn_id_footguns(
        row_id.is_some(),
        selection_lens.is_some() && (sort_change.is_some() || filter_change.is_some()),
        expandable.is_some(),
    );

    // Default the row-id projector to each row's slice position when the
    // host doesn't supply one. Correct only for a static (unsorted,
    // unfiltered) grid; documented on `DataGrid::row_id`. Boxed once here
    // so the body closure captures a single uniform `RowIdFn`.
    let row_id: IdSource<R> = match row_id {
        Some(f) => IdSource::Explicit(f),
        None => IdSource::Position,
    };

    let DecomposedColumns {
        render_slots,
        text_projectors,
        sortable,
        filterable,
    } = decompose_columns(columns, &column_widths);

    // --- Header row. Sortable columns get a clickable header that
    //     emits the clicked column to the host (which cycles the sort and
    //     re-derives its order), plus an arrow on the active column;
    //     columns with an active filter get a persistent accent + marker.
    //     When resize is enabled the header ColumnStrip itself owns the
    //     column-boundary grab zones + separators (like masonry's Split
    //     owns its bar) — no overlay, so cell hover/sort are untouched.
    let header_ctx = HeaderCtx {
        sort,
        sort_change: sort_change.as_ref(),
        theme: &theme,
    };
    let header_widths: Vec<f64> = render_slots.iter().map(|s| s.width).collect();
    let header_cells: Vec<Box<AnyWidgetView<State, Action>>> = render_slots
        .iter()
        .enumerate()
        .map(|(idx, slot)| {
            let filtered = filter.get(&slot.id).is_some();
            header_cell(slot, sortable[idx], filtered, &header_ctx)
        })
        .collect();
    // ColumnStrip places each cell at an authoritative x (= cumulative
    // width), so the header lines up with the body/filter strips by
    // construction. Made resizable when a resize callback is supplied.
    let mut header_strip = column_strip(header_widths, row_height, header_cells);
    // Move (don't clone) the resize callback: this is its only use, so a
    // clone would bump the `Arc` refcount only to drop the original.
    if let Some(width_change) = width_change {
        let style = SeparatorStyle {
            line: theme.palette.border,
            active: theme.palette.accent,
        };
        // The strip reports a *positional* resize index (it's a layout
        // widget — it knows columns by slot, not identity). Translate it
        // to the column's stable id here so the host's width override
        // stays attached across reorder/hide.
        let resize_ids: Vec<ColumnId> = render_slots.iter().map(|s| s.id.clone()).collect();
        header_strip = header_strip.resizable(style, move |state: &mut State, col, new_width| {
            if let Some(id) = resize_ids.get(col) {
                width_change(state, id.clone(), new_width)
            } else {
                Action::default()
            }
        });
    }
    let header = sized_box(header_strip)
        .background_color(theme.palette.surface_2)
        .border(theme.palette.border, Length::px(1.0));

    let body = build_body(BodyParams {
        row_count,
        row_height,
        theme,
        render_slots: Arc::clone(&render_slots),
        rows: rows.clone(),
        selection_lens: selection_lens.clone(),
        row_id: row_id.clone(),
        scroll,
        expandable,
    });

    // Build the filter-input row only when filtering is configured and
    // at least one column is filterable.
    let filter_row = filter_change.as_ref().and_then(|on_change| {
        filterable.iter().any(|&f| f).then(|| {
            let widths: Vec<f64> = render_slots.iter().map(|s| s.width).collect();
            let ids: Vec<ColumnId> = render_slots.iter().map(|s| s.id.clone()).collect();
            build_filter_row(&widths, &ids, &filterable, &filter, on_change, &theme)
        })
    });
    let stack = assemble_grid_stack(header, filter_row, body);

    // --- Horizontal scroll: wrap the whole header+filter+body stack in
    //     a horizontal-only scroll so columns wider than the viewport
    //     are reachable. Because header/filter/body are a single child
    //     subtree, they share the horizontal offset automatically — no
    //     manual sync. Vertical virtualization stays inside the body
    //     (`constrain_vertical` leaves the vertical axis to it).
    let inner = scroll_container(stack.boxed())
        .constrain_vertical(true)
        .render(&theme);

    // --- Wrap in CopyOnShortcut so Ctrl/Cmd+C dumps the
    //     selection-projected TSV. The wrapper captures the text
    //     projectors Arc, the data accessor, and the selection lens
    //     so it can recompute the payload on every rebuild.
    CopyOnShortcutView {
        child: inner,
        text_projectors,
        rows,
        selection_lens,
        row_id,
        phantom: PhantomData,
    }
}

// --- MARK: Integer-domain boundaries -----------------------------------
//
// Two crates impose integer types on a row's *position*:
//
// - `usize` is the native slice-position form (`data.get`, enumerate).
// - `i64` is forced by xilem's `virtual_scroll`, whose range bound *and*
//   callback index are `i64`.
//
// Separately, a row's **stable id** is a `u64` ([`SelectionState`] is
// keyed by it). Id and position are *different* quantities now that the
// host owns order — they coincide only under the position fallback
// ([`IdSource::Position`]), which is the one place `usize → u64`
// happens (`position_fallback_id`). We never convert an id *back* to a
// position by casting (an id isn't a position); the copy path resolves
// id→row by scanning instead. (Column indices are a separate domain,
// uniformly `usize` — see `filter`/`sort`.)
//
// The conversions go through named helpers so the casts aren't scattered.
// Each saturates an out-of-range value to its type's max; downstream
// lookups then treat that as "past the end." On 64-bit targets the casts
// are lossless, but the checked form keeps us correct on 32-bit and
// satisfies `clippy::pedantic` uniformly.

// The integer-domain converters (`scroll_range_end`, `scroll_idx_to_slice`,
// `position_fallback_id`) now live in `crate::collection::ids`.

// --- MARK: TSV projection ----------------------------------------------

/// Builds the clipboard TSV for the current selection.
///
/// Iterates `data` **in its current (display) order** and emits a line
/// for each row whose [`row_id`](IdSource)-derived id is selected.
/// Walking the data (rather than the selection set) means the copy comes
/// out in the on-screen order the host arranged — not in id order — which
/// is what a user pasting into a spreadsheet expects.
fn project_tsv<R>(
    text_projectors: &[Option<TextProjector<R>>],
    data: &[R],
    selection: &SelectionState,
    row_id: &IdSource<R>,
) -> Option<String> {
    if selection.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut first_row = true;
    // Debug-only row-id uniqueness check, free-riding on this scan (which
    // only runs on copy, never per frame). If two distinct rows project
    // the *same* selected id, the `row_id` contract is violated and copy
    // would emit both for one logical selection — flag it loudly in debug.
    #[cfg(debug_assertions)]
    let mut seen_selected_ids = std::collections::BTreeSet::<u64>::new();
    for (pos, row) in data.iter().enumerate() {
        let id = row_id.id_of(pos, row);
        if !selection.contains(id) {
            continue;
        }
        #[cfg(debug_assertions)]
        debug_assert!(
            seen_selected_ids.insert(id),
            "data_grid: row_id is not unique — id {id} maps to more than \
             one row; selection/copy will misbehave (see DataGrid::row_id)"
        );
        if !first_row {
            out.push('\n');
        }
        first_row = false;
        let mut first_col = true;
        for projector in text_projectors {
            if !first_col {
                out.push('\t');
            }
            first_col = false;
            if let Some(p) = projector {
                let cell = p(row);
                // Literal-escape tab/newline so they don't break the
                // row × column layout in the spreadsheet target.
                out.push_str(&cell.replace('\t', "\\t").replace('\n', "\\n"));
            }
        }
    }
    // Selected ids that aren't in the current (e.g. filtered) view simply
    // don't appear — copy reflects what's visible, matching the grid's
    // host-owned-view model.
    (!first_row).then_some(out)
}

// --- MARK: CopyOnShortcutView ------------------------------------------

/// Internal wrapper view for the [`CopyOnShortcut`] widget. Holds the row
/// accessor, text projectors, and selection lens so it can project the
/// clipboard TSV from the current selection — **lazily**, only when the
/// widget emits [`CopyRequested`] (an actual Ctrl/Cmd+C), never per rebuild.
struct CopyOnShortcutView<V, R, State, Action> {
    child: V,
    text_projectors: Arc<Vec<Option<TextProjector<R>>>>,
    rows: RowsFn<State, R>,
    selection_lens: Option<SelectionLens<State>>,
    row_id: IdSource<R>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<V, R, State, Action> ViewMarker for CopyOnShortcutView<V, R, State, Action> {}

impl<V, R, State, Action> View<State, Action, ViewCtx> for CopyOnShortcutView<V, R, State, Action>
where
    V: WidgetView<State, Action>,
    R: 'static,
    State: 'static,
    Action: Default + 'static,
{
    type Element = Pod<CopyOnShortcut>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = CopyOnShortcut::new(child_pod.new_widget);
        // Register as an action source so the widget's `CopyRequested`
        // routes back to `message` below (mirrors `ClickableRow`).
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        // No per-rebuild payload work: the TSV is projected lazily, only when
        // a `CopyRequested` action arrives (see `message`). This is what keeps
        // the O(rows) selection scan off the per-frame path.
        let mut child = CopyOnShortcut::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut child = CopyOnShortcut::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child.downcast());
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // `CopyRequested` is addressed to *this* wrapper's widget, so it
        // arrives fully routed (empty remaining path). Every other message —
        // header clicks, filter edits, row clicks, `VirtualScrollAction` —
        // is bound for a descendant and passes through here with a non-empty
        // path; it must be forwarded untouched. Probing it with
        // `take_message` would panic ("message has not reached its target"),
        // so we gate on the empty path first (same guard as
        // `CollectionBodyView::message`).
        if message.remaining_path().is_empty() {
            // Our widget's only action is the copy request. Project the TSV
            // from the *current* selection now — the only place this O(rows)
            // scan runs — and write it via the widget's `MutateCtx`.
            if message.take_message::<CopyRequested>().is_some()
                && let Some(payload) = self.compute_payload(app_state)
            {
                CopyOnShortcut::write_clipboard(&mut element, payload);
            }
            return MessageResult::Action(Action::default());
        }
        let mut child = CopyOnShortcut::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}

impl<V, R, State, Action> CopyOnShortcutView<V, R, State, Action>
where
    R: 'static,
    State: 'static,
{
    fn compute_payload(&self, app_state: &mut State) -> Option<String> {
        // Called only when the widget emits `CopyRequested` (an actual
        // Ctrl/Cmd+C) — never per rebuild — so the O(rows) scan in
        // `project_tsv` is off the hot path. No selection lens → nothing to
        // copy.
        let selection_lens = self.selection_lens.as_ref()?;
        // Cheap borrow first: with nothing selected (the common case
        // during scroll / data-tick rebuilds) skip the snapshot clone
        // and the row scan entirely.
        if (**selection_lens)(app_state).is_empty() {
            return None;
        }
        // We need both `&[R]` and `&SelectionState` simultaneously,
        // but the lenses return references whose lifetimes overlap
        // app_state. Snapshot the selection first, then look up rows.
        let selection_snapshot = (**selection_lens)(app_state).clone();
        let data = (*self.rows)(app_state);
        project_tsv(
            &self.text_projectors,
            data,
            &selection_snapshot,
            &self.row_id,
        )
    }
}

// --- MARK: CELL ALIGNMENT ---------------------------------------------

/// Wraps a cell view in a fixed-width `sized_box` with the cell's
/// content packed via a single-child `flex_row` whose
/// `main_axis_alignment` is derived from [`CellAlign`].
///
/// Returning a boxed `AnyWidgetView` keeps the concrete type out of
/// the call sites in the header / body builders (which already
/// erase further).
fn aligned_cell<State: 'static, Action: 'static>(
    inner: Box<AnyWidgetView<State, Action>>,
    width: f64,
    align: CellAlign,
) -> Box<AnyWidgetView<State, Action>> {
    let aligned = flex_row((flex_item(inner, 0.0),))
        .main_axis_alignment(align_to_main(align))
        .cross_axis_alignment(CrossAxisAlignment::Center);
    Box::new(sized_box(aligned).fixed_width(Length::px(width)))
}

// --- MARK: HEADER CELL -------------------------------------------------

/// Shared (non-per-column) inputs for [`header_cell`], grouped to keep
/// the call short and under the argument-count lint.
struct HeaderCtx<'a, State, Action> {
    sort: SortState,
    sort_change: Option<&'a SortChange<State, Action>>,
    theme: &'a Theme,
}

/// Builds one header cell as a fixed-width flex child.
///
/// Sortable columns (`sortable == true`) are wrapped in
/// [`clickable_header`] so a click emits the column through
/// `sort_change` (the host cycles its [`SortState`] and re-derives its
/// order), and the active sort column gains an ascending/descending
/// arrow. A column with an active filter (`filtered == true`) is drawn
/// in the theme accent with a trailing marker, so a filtered view is
/// always unmistakable. (Resize is owned by the header
/// [`ColumnStrip`](super::column_strip::ColumnStrip) itself — it
/// hit-tests a grab zone at each column boundary — not by the cell, so
/// `header_cell` stays resize-agnostic.) Non-sortable columns render an
/// inert label.
fn header_cell<State, R, Action>(
    slot: &ColumnRender<R, State>,
    sortable: bool,
    filtered: bool,
    ctx: &HeaderCtx<'_, State, Action>,
) -> Box<AnyWidgetView<State, Action>>
where
    State: 'static,
    R: 'static,
    Action: Default + 'static,
{
    let theme = ctx.theme;
    let mut title = match ctx.sort.direction_for(&slot.id) {
        Some(SortDirection::Ascending) => format!("{}  ▲", slot.title),
        Some(SortDirection::Descending) => format!("{}  ▼", slot.title),
        None => slot.title.clone(),
    };
    // Multi-sort priority badge: when more than one column is sorted,
    // append this column's 1-based priority after its arrow (1 = primary,
    // 2+ = tiebreakers) so the sort order is legible — the convention
    // AG Grid (`sortIndex`) and Kendo (`showIndexes`) use. Hidden for a
    // single-column sort, where there's no ambiguity.
    if ctx.sort.len() > 1
        && let Some(priority) = ctx.sort.priority_of(&slot.id)
    {
        use std::fmt::Write as _;
        let _ = write!(title, " {}", priority + 1);
    }
    // Persistent filter indicator: a filtered column gets a trailing
    // marker and the theme accent color, so a hidden-data view can't be
    // mistaken for the full set (even after the trigger loses focus).
    if filtered {
        title.push_str("  ●");
    }
    let title_color = if filtered {
        theme.palette.accent
    } else {
        theme.palette.text_muted
    };
    let header_label = label(title)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(title_color);

    // The enclosing ColumnStrip cell is exactly `slot.width` and fills
    // it. The resize grab zone + separator are owned by the strip itself
    // (it hit-tests the trailing edge), so the cell content needn't
    // reserve any width and the hover highlight spans the full column.
    let cell = aligned_cell(Box::new(header_label), slot.width, slot.align);

    // Interactive (sort) only when the column is sortable *and* a sort
    // callback is available; otherwise an inert label. The clickable
    // wrapper spans the full column width, so the hover highlight covers
    // the whole header cell. The click emits the column to the host,
    // which cycles the sort and re-derives its order — the grid doesn't
    // reorder data itself.
    match (sortable, ctx.sort_change) {
        (true, Some(on_sort)) => {
            let on_sort = Arc::clone(on_sort);
            let id = slot.id.clone();
            Box::new(clickable_header(
                cell,
                theme.palette.border_strong,
                move |state: &mut State, multi: bool| on_sort(state, id.clone(), multi),
            ))
        }
        _ => cell,
    }
}

// --- MARK: BODY --------------------------------------------------------

/// Inputs for [`build_body`], grouped into a struct to keep the call
/// readable (and under the argument-count lint).
struct BodyParams<State, R> {
    row_count: u64,
    row_height: f64,
    theme: Theme,
    render_slots: Arc<Vec<ColumnRender<R, State>>>,
    rows: RowsFn<State, R>,
    selection_lens: Option<SelectionLens<State>>,
    row_id: IdSource<R>,
    /// Pending programmatic-scroll request snapshot (see [`ScrollState`]).
    scroll: ScrollState,
    /// Tree-column config when [`DataGrid::expandable`] is set; `None`
    /// leaves the body a flat grid.
    expandable: Option<ExpandableConfig<State, R>>,
}

/// Builds the virtualized body. The host supplies rows **already in
/// display order** (filtered then sorted host-side), so virtual position
/// *is* slice position — the body does no reordering. Each row's stable
/// id (via [`IdSource`]) drives selection and click routing — now handled
/// by the collection substrate's `collection_body` — so a selection follows
/// its rows across host reordering.
fn build_body<State, R, Action>(
    params: BodyParams<State, R>,
) -> impl WidgetView<State, Action> + use<State, R, Action>
where
    State: 'static,
    R: 'static,
    Action: Default + 'static,
{
    let BodyParams {
        row_count,
        row_height,
        theme,
        render_slots,
        rows,
        selection_lens,
        row_id,
        scroll,
        expandable,
    } = params;
    // Column widths are identical for every row and don't change between
    // rebuilds of this body, so compute them once and share the `Arc` into
    // the per-row closure rather than re-deriving from `render_slots` on
    // every visible row; `column_strip` takes the `Arc` directly, so each
    // row costs a refcount bump, not a `Vec` allocation.
    let widths: Arc<Vec<f64>> = Arc::new(render_slots.iter().map(|s| s.width).collect());

    // Expandable rows reserve a leading defer-to-chevron zone so a click on
    // the chevron toggles the row instead of selecting it (see
    // `ClickableRow::leading_hit`). A parent reserves *only* the chevron's
    // box — offset by the depth indent, `disclosure_hit_width` wide — so the
    // blank indent gutter to its left still selects, exactly like a leaf's
    // indent (leaves reserve nothing). This keeps selection consistent between
    // a nested parent and a leaf sibling; a zone anchored at the left edge
    // would instead make the gutter a dead zone that grows with tree depth.
    let leading_hit_zone: Option<LeadingHitZoneFn<R>> = expandable.as_ref().map(|cfg| {
        let has_children = Arc::clone(&cfg.has_children);
        let depth = Arc::clone(&cfg.depth);
        let density = theme.density;
        let f: LeadingHitZoneFn<R> = Arc::new(move |row: &R| {
            has_children(row).then(|| LeadingHitZone {
                offset: f64::from(depth(row)) * tree_indent_step(&density),
                width: disclosure_hit_width(&density),
            })
        });
        f
    });

    // Per-row CONTENT only: the cell strip. `collection_body` wraps this
    // with the selection background (`surface_2` when selected) and the
    // `clickable_row` selection routing, so this closure neither styles
    // selection nor handles clicks. ColumnStrip gives every body row the
    // same authoritative column x-positions as the header/filter strips;
    // the shared width list is handed over as an `Arc` clone — a refcount
    // bump, not a per-row `Vec` allocation. When expansion is configured the
    // first cell becomes the tree cell (indent + chevron).
    let render_row: RenderRow<State, R> = {
        let render_slots = Arc::clone(&render_slots);
        let widths = Arc::clone(&widths);
        let row_id = row_id.clone();
        Arc::new(
            move |row: &R, _selected: bool, theme: &Theme| -> Box<AnyWidgetView<State>> {
                let cells: Vec<Box<AnyWidgetView<State>>> = render_slots
                    .iter()
                    .enumerate()
                    .map(|(idx, slot)| {
                        let content = (slot.render)(row, theme);
                        // The first column is the tree column when expansion
                        // is configured: prepend a depth indent + chevron (or
                        // an aligned spacer for a leaf) to its content.
                        if idx == 0
                            && let Some(cfg) = expandable.as_ref()
                        {
                            return tree_cell(content, row, slot, cfg, &row_id, theme);
                        }
                        // Cell content only; ColumnStrip owns the width. Keep
                        // the per-cell alignment wrapper so Start/Center/End
                        // still position text within the cell.
                        aligned_cell(content, slot.width, slot.align)
                    })
                    .collect();
                Box::new(column_strip(Arc::clone(&widths), row_height, cells))
            },
        )
    };

    collection_body(CollectionBodyParams {
        item_count: row_count,
        items: rows,
        id_source: row_id,
        selection_lens,
        scroll,
        lazy: None::<Lazy<State, Action>>,
        render_row,
        leading_hit_zone,
        theme,
    })
}

/// Builds the tree-column cell for an expandable row: a depth indent, then
/// either an interactive disclosure chevron (parent) or an aligned spacer
/// (leaf), then the column's own rendered content — all inside the column's
/// fixed-width slot.
///
/// The chevron sits in a fixed `disclosure_hit_width` box that the row
/// reserves as its leading defer-to-child zone, so clicking it toggles
/// (via `on_toggle`) rather than selects. A leaf reserves the same width as
/// an inert spacer so its content lines up under a parent's content.
fn tree_cell<State: 'static, R>(
    content: Box<AnyWidgetView<State>>,
    row: &R,
    slot: &ColumnRender<R, State>,
    cfg: &ExpandableConfig<State, R>,
    row_id: &IdSource<R>,
    theme: &Theme,
) -> Box<AnyWidgetView<State>> {
    let expanded = (cfg.is_expanded)(row);
    let has_children = (cfg.has_children)(row);
    let depth = (cfg.depth)(row);
    let indent = f64::from(depth) * tree_indent_step(&theme.density);
    let hit_w = disclosure_hit_width(&theme.density);

    // The chevron slot: an interactive toggle for a parent, or a fixed-width
    // gap for a leaf so its content still lines up under a parent's.
    let chevron: AnyFlexChild<State, ()> = if has_children {
        // `id_of` ignores the position under an explicit `row_id` (the
        // required config for expansion); the `0` placeholder only matters
        // under the position fallback, which we already assert/warn about.
        let id = row_id.id_of(0, row);
        let on_toggle = Arc::clone(&cfg.on_toggle);
        // `aria-level` is 1-based, so a depth-0 root row is level 1.
        let level = usize::from(depth) + 1;
        // Center the chevron glyph inside its fixed hit box.
        let centered = flex_row((flex_item(
            disclosure_chevron::<State, ()>(expanded, theme),
            0.0,
        ),))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center);
        let toggle = disclosure_toggle(
            centered,
            expanded,
            level,
            theme,
            move |state: &mut State| {
                on_toggle(state, id);
            },
        );
        let toggle: Box<AnyWidgetView<State>> =
            Box::new(sized_box(toggle).fixed_width(Length::px(hit_w)));
        flex_item(toggle, 0.0).into()
    } else {
        AnyFlexChild::Spacer(FlexSpacer::Fixed(Length::px(hit_w)))
    };

    // [indent][chevron | spacer][content], vertically centered. The indent
    // and leaf placeholder are fixed-width gaps (`FlexSpacer::Fixed`).
    let tree_row = flex_row(vec![
        AnyFlexChild::Spacer(FlexSpacer::Fixed(Length::px(indent))),
        chevron,
        flex_item(content, 0.0).into(),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    aligned_cell(Box::new(tree_row), slot.width, slot.align)
}

// --- MARK: STACK ASSEMBLY ----------------------------------------------

/// Stacks header → (optional filter row) → body into the grid column.
///
/// The body flexes to fill the height the grid is given; the header and
/// filter row keep their fixed heights. A virtualized grid must fill a
/// *bounded* viewport (it can't size itself to the full row count), so
/// callers place it in a bounded-height slot — e.g.
/// `sized_box(grid).flex(1.0)`; an unbounded parent falls back to the
/// body's intrinsic size.
fn assemble_grid_stack<State, Action, H, F, B>(
    header: H,
    filter_row: Option<F>,
    body: B,
) -> impl WidgetView<State, Action> + use<State, Action, H, F, B>
where
    State: 'static,
    Action: 'static,
    H: WidgetView<State, Action>,
    F: WidgetView<State, Action>,
    B: WidgetView<State, Action>,
{
    let mut children: Vec<AnyFlexChild<State, Action>> = Vec::with_capacity(3);
    children.push(flex_item(header, 0.0).into());
    if let Some(filter_row) = filter_row {
        children.push(flex_item(filter_row, 0.0).into());
    }
    children.push(flex_item(body, 1.0).into());
    flex_col(children).cross_axis_alignment(CrossAxisAlignment::Start)
}

// --- MARK: FILTER ROW --------------------------------------------------

/// Builds the per-column filter-input row shown beneath the header.
///
/// Each filterable column gets a `text_input` seeded from its current
/// query; editing it calls `on_change(state, column, query)` so the
/// host updates its [`FilterState`] and re-derives the filtered view.
/// Non-filterable columns render a blank slot of the same width so the
/// inputs line up under their columns.
fn build_filter_row<State, Action>(
    widths: &[f64],
    ids: &[ColumnId],
    filterable: &[bool],
    filter: &FilterState,
    on_change: &FilterChange<State, Action>,
    theme: &Theme,
) -> impl WidgetView<State, Action> + use<State, Action>
where
    State: 'static,
    Action: 'static,
{
    let cells: Vec<Box<AnyWidgetView<State, Action>>> = (0..widths.len())
        .map(|idx| -> Box<AnyWidgetView<State, Action>> {
            if filterable[idx] {
                let id = ids[idx].clone();
                let current = filter.get(&id).unwrap_or_default().to_string();
                let on_change = Arc::clone(on_change);
                // Default text size: masonry renders the placeholder as a
                // separate Label at the default font size (it doesn't
                // inherit `text_size`), so overriding it would clip the
                // placeholder. ColumnStrip force-sizes the cell to the
                // column width, so the input can't overflow its column —
                // this is what finally fixed the filter-alignment bug.
                let input = text_input(current, move |state: &mut State, text: String| {
                    (*on_change)(state, id.clone(), text)
                })
                .text_color(theme.palette.text)
                .placeholder("Filter");
                Box::new(input)
            } else {
                Box::new(label(""))
            }
        })
        .collect();
    // Filter row is deliberately taller than data rows: filter_row_height()
    // adds pad_v of headroom on top of row_height so the text_input isn't
    // clipped. ColumnStrip enforces both per-column width and row height.
    sized_box(column_strip(
        widths.to_vec(),
        filter_row_height(&theme.density),
        cells,
    ))
    .background_color(theme.palette.surface)
    .border(theme.palette.border, Length::px(1.0))
}

/// Height of the filter-input row: one density row plus a `pad_v` of
/// headroom so the `text_input` (font + its internal padding) isn't
/// clipped (24 + 6 = the pre-token 30 px at balanced).
fn filter_row_height(density: &Density) -> f64 {
    f64::from(density.row_height) + f64::from(density.pad_v)
}

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::sync::Arc;

    use masonry::core::{NewWidget, WidgetRef};
    use masonry::testing::TestHarness;
    use masonry::widgets::Flex;
    use xilem::ViewCtx;
    use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};

    use super::{data_grid, decompose_columns, project_tsv};
    use crate::Theme;
    use crate::collection::{CollectionBodyWidget, IdSource, SelectionState};
    use crate::components::data_grid::column::{CellAlign, TextProjector, text_column};
    use crate::components::data_grid::width::ColumnWidths;

    /// Row id == the row value itself, so test slices read naturally:
    /// `[10, 20, 30]` are rows with ids 10/20/30 in that display order.
    fn id_is_value() -> IdSource<u64> {
        IdSource::Explicit(Arc::new(|r: &u64| *r))
    }

    /// A single text projector that stringifies the `u64` row.
    fn value_projectors() -> Vec<Option<TextProjector<u64>>> {
        vec![Some(Box::new(|r: &u64| r.to_string()))]
    }

    #[test]
    fn default_row_height_follows_density() {
        use crate::theme::Density;

        use super::{default_row_height, filter_row_height};

        // Balanced reproduces the pre-token constants exactly.
        assert!((default_row_height(&Density::balanced()) - 24.0).abs() < 1e-6); // was DEFAULT_ROW_HEIGHT
        assert!((filter_row_height(&Density::balanced()) - 30.0).abs() < 1e-6); // was FILTER_ROW_HEIGHT
        assert!(default_row_height(&Density::compact()) < default_row_height(&Density::balanced()));
        assert!(filter_row_height(&Density::compact()) < filter_row_height(&Density::balanced()));
    }

    /// EVIDENCE (not a correctness check): the clipboard TSV projection scans
    /// the entire dataset, so its cost scales with total row count — a single
    /// selected row is enough to make it O(rows). This *used* to run on every
    /// rebuild whenever a selection existed (the cause of the grid getting
    /// slower with more rows); it now runs only when the user actually presses
    /// Ctrl/Cmd+C (see `CopyOnShortcutView::message`). This measures that
    /// per-copy cost — the work that is no longer on the per-frame path.
    ///
    /// Run with:
    /// `cargo test --all-features -- --ignored --nocapture project_tsv_scales_with_row_count`
    #[test]
    #[ignore = "timing evidence, not a correctness check; run with --ignored --nocapture"]
    #[expect(
        clippy::cast_precision_loss,
        reason = "informational timing print; ns/sample values are far below f64's exact-integer range"
    )]
    fn project_tsv_scales_with_row_count() {
        use std::hint::black_box;
        use std::time::Instant;

        // A wide-ish blotter: 8 text columns, like a real grid.
        let projectors: Vec<Option<TextProjector<u64>>> = (0..8)
            .map(|c| -> Option<TextProjector<u64>> {
                Some(Box::new(move |r: &u64| format!("c{c}:{r}")))
            })
            .collect();

        for &n in &[1_000_u64, 10_000, 100_000, 1_000_000] {
            let data: Vec<u64> = (0..n).collect();
            // Select exactly ONE row — the worst case for "wasted" work: the
            // whole dataset is scanned to find it on every rebuild.
            let mut sel = SelectionState::new();
            sel.replace_with(0);

            // Warm up, then take a median over a few iterations.
            for _ in 0..3 {
                black_box(project_tsv(&projectors, &data, &sel, &id_is_value()));
            }
            let mut samples: Vec<u128> = (0..9)
                .map(|_| {
                    let start = Instant::now();
                    black_box(project_tsv(&projectors, &data, &sel, &id_is_value()));
                    start.elapsed().as_nanos()
                })
                .collect();
            samples.sort_unstable();
            let median_us = samples[samples.len() / 2] as f64 / 1_000.0;
            println!("project_tsv: rows={n:>9} selected=1 → {median_us:>8.1}µs per copy");
        }
    }

    #[test]
    fn project_tsv_emits_selected_rows_in_display_order() {
        // Display order is the slice order; selection holds ids 30 and 10.
        // Copy must come out in display order (30 then 10), not id order.
        let data = [30_u64, 20, 10];
        let mut sel = SelectionState::new();
        sel.replace_with(30);
        sel.extend_range([30, 10]);
        let tsv = project_tsv(&value_projectors(), &data, &sel, &id_is_value());
        assert_eq!(tsv.as_deref(), Some("30\n10"));
    }

    #[test]
    fn project_tsv_is_none_when_nothing_selected() {
        let data = [1_u64, 2, 3];
        let sel = SelectionState::new();
        assert_eq!(
            project_tsv(&value_projectors(), &data, &sel, &id_is_value()),
            None
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "row_id is not unique")]
    fn project_tsv_debug_asserts_unique_row_ids() {
        // Two rows project the same id (5) and that id is selected — the
        // contract is violated, so debug builds must trip the assertion.
        let data = [5_u64, 5];
        let mut sel = SelectionState::new();
        sel.replace_with(5);
        let _ = project_tsv(&value_projectors(), &data, &sel, &id_is_value());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "column id Price is not unique")]
    fn decompose_debug_asserts_unique_column_ids() {
        // Two columns derive the same id "Price" from their titles (no
        // explicit `.id`), so column state would collide — a debug build
        // must trip the uniqueness assertion. (Release builds warn once
        // instead; that path can't be asserted in a unit test.)
        let cols = vec![
            text_column::<u64, (), _>("Price", 80.0, CellAlign::End, |r: &u64| r.to_string()),
            text_column::<u64, (), _>("Price", 80.0, CellAlign::End, |r: &u64| r.to_string()),
        ];
        let _ = decompose_columns(cols, &ColumnWidths::new());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "every chevron toggles the same")]
    fn expandable_without_row_id_debug_asserts() {
        // `.expandable(...)` without `.row_id(...)` is non-functional (every
        // chevron resolves to id 0), so a debug build must fail loudly when
        // the grid materializes — release builds warn once instead.
        let cols = vec![text_column::<u64, (), _>(
            "N",
            80.0,
            CellAlign::Start,
            |r: &u64| r.to_string(),
        )];
        let _ = data_grid::<(), u64, ()>(cols)
            .rows(|_state: &()| &[][..])
            .expandable(
                |_r: &u64| false,
                |_r: &u64| false,
                |_r: &u64| 0,
                |_s: &mut (), _id: u64| {},
            )
            .render(&Theme::default());
    }

    /// A [`RawProxy`] that drops every message — tests build the view
    /// directly and never expect a proxied message back.
    #[derive(Debug)]
    struct NoopProxy;

    impl RawProxy for NoopProxy {
        fn send_message(
            &self,
            _path: Arc<[ViewId]>,
            _message: SendMessage,
        ) -> Result<(), ProxyError> {
            Ok(())
        }
        fn dyn_debug(&self) -> &dyn fmt::Debug {
            self
        }
    }

    /// Depth-first search for a [`CollectionBodyWidget`] in the tree.
    fn find_collection_body(
        widget: WidgetRef<'_, dyn masonry::core::Widget>,
    ) -> Option<WidgetRef<'_, CollectionBodyWidget>> {
        if let Some(body) = widget.downcast::<CollectionBodyWidget>() {
            return Some(body);
        }
        widget.children().into_iter().find_map(find_collection_body)
    }

    /// `data_grid`'s body is now the collection substrate's
    /// [`CollectionBodyWidget`], which is what grants it arrow-key row
    /// navigation (the navigation behavior itself is exercised
    /// exhaustively by `collection::body`'s widget-level arrow-key tests).
    /// This integration test materializes a real `data_grid` view and
    /// asserts the substrate body widget is present in the rendered tree —
    /// the seam that wires `data_grid` onto that navigation.
    #[test]
    fn data_grid_body_is_the_collection_substrate_widget() {
        struct State {
            rows: Vec<u64>,
        }

        let mut state = State {
            rows: vec![10, 20, 30],
        };

        let columns = vec![text_column::<u64, State, _>(
            "Value",
            80.0,
            CellAlign::End,
            |r: &u64| r.to_string(),
        )];
        let view = data_grid::<State, u64, ()>(columns)
            .rows(|s: &State| &s.rows[..])
            .row_count(3)
            .render(&Theme::default());

        let proxy: Arc<dyn RawProxy> = Arc::new(NoopProxy);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
        );
        let mut ctx = ViewCtx::new(proxy, runtime);
        let (pod, _view_state) = view.build(&mut ctx, &mut state);

        let root = Flex::column().with_fixed(pod.new_widget);
        let harness =
            TestHarness::create(masonry::theme::default_property_set(), NewWidget::new(root));

        assert!(
            find_collection_body(harness.root_widget().as_dyn()).is_some(),
            "data_grid's body should be a CollectionBodyWidget (grants arrow-key nav)",
        );
    }

    /// sort/filter/resize callbacks return the host's Action type, which the
    /// rendered grid view is generic over.
    #[test]
    fn data_grid_callbacks_return_the_host_action() {
        use xilem::WidgetView;

        use crate::components::data_grid::filter::FilterState;
        use crate::components::data_grid::sort::SortState;

        #[derive(Default, Debug)]
        struct Reorder;
        struct State {
            rows: Vec<u64>,
        }
        fn assert_widget_view<V: WidgetView<State, Reorder>>(_: &V) {}

        let columns = vec![text_column::<u64, State, _>(
            "Value",
            80.0,
            CellAlign::End,
            |r: &u64| r.to_string(),
        )];
        let view = data_grid::<State, u64, Reorder>(columns)
            .rows(|s: &State| &s.rows[..])
            .row_count(3)
            .sort(SortState::new(), |_s, _col, _multi| Reorder)
            .filter(FilterState::new(), |_s, _col, _q| Reorder)
            .on_column_resize(|_s, _col, _w| Reorder)
            .render(&Theme::default());
        assert_widget_view(&view);
    }
}
