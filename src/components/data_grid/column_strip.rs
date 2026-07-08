//! `ColumnStrip` — a horizontal row of cells placed at **authoritative**
//! x-positions derived from a shared list of column widths.
//!
//! ## Why this exists (new pattern, deliberate)
//!
//! The grid's header, body, and filter rows must share *identical*
//! column geometry. Building each row as an independent `flex_row` of
//! fixed-width cells does **not** guarantee that: with sparse content
//! the rows visibly drift and gap (confirmed empirically with bare
//! colored tiles). Flex distributes leftover main-axis space, and how it
//! does so depends on the laying-out parent — so "same per-cell widths"
//! did not produce "same x-positions" across the three rows.
//!
//! `ColumnStrip` removes the guesswork: it owns the per-column widths
//! and, in [`layout`](Widget::layout), force-runs each child at exactly
//! `(width[i], row_height)` and places it at `x = sum(width[0..i])`.
//! Every strip built from the same width list is therefore pixel-aligned
//! *by construction*, independent of content or parent. Its own width is
//! the sum of column widths (so the horizontal-scroll extent is right);
//! its height is the fixed `row_height`.
//!
//! Structurally this mirrors the crate's existing multi-child view
//! [`flex_wrap`](crate::layout::flex_wrap) (a `CollectionWidget` + a
//! `ViewSequence`/`ElementSplice` + a blanket sequence-marker trait), so
//! it composes with tuples of mixed cell views just like `flex_row(..)`.
//! It is *not* a new architectural precedent — only a new layout rule.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CollectionWidget, CursorIcon, EventCtx, LayoutCtx,
    MeasureCtx, NewWidget, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PointerUpdate, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

/// Width (px) of the parent-owned grab zone at each column's trailing
/// edge. On a *resizable* strip the zone must stay uncovered so the
/// strip itself receives pointer events there (the way masonry's `Split`
/// reserves its bar region); it's reserved on **every** strip — resizable
/// or not — so End-aligned content in header/body/filter rows lands at
/// the same x by construction. Also the hover tolerance. A resize hit-target
/// half-width; hit targets must not shrink at compact density, so this
/// doesn't scale with density.
const GRAB_ZONE: f64 = 6.0;

/// The width a cell is laid out at inside its column slot: the column
/// width minus the trailing [`GRAB_ZONE`], floored at zero. Identical
/// for every strip so cell content (notably End-aligned text) aligns
/// across the header/body/filter rows regardless of which strip is
/// resizable. Column *pitch* is unaffected — only the child's size.
fn cell_layout_width(width: f64) -> f64 {
    (width - GRAB_ZONE).max(0.0)
}

/// Action emitted by a resizable [`ColumnStrip`] while a column boundary
/// is dragged: the affected column and its proposed new absolute width
/// (un-clamped — the host clamps via `ColumnWidths`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnResize {
    /// Index of the column whose trailing boundary is being dragged.
    pub col: usize,
    /// Proposed new width for that column, in px (pre-clamp).
    pub new_width: f64,
}

/// In-flight boundary drag, snapshotted on pointer-down so mid-drag
/// rebuilds can't shift the reference point.
#[derive(Clone, Copy, Debug)]
struct ColumnDrag {
    col: usize,
    base_width: f64,
    start_x: f64,
}

/// Colors for the resizable strip's column separators.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeparatorStyle {
    /// Idle separator line color.
    pub line: Color,
    /// Separator color while its boundary is hovered or dragged.
    pub active: Color,
}

/// A row of cells laid out at explicit, cumulative x-positions.
///
/// Widths are stored parallel to `children` (same length/order). Use
/// [`Self::with`] / [`CollectionWidget::add`] to append cells; per-column
/// widths are set at construction and adjusted via [`Self::set_width`].
pub struct ColumnStrip {
    children: Vec<WidgetPod<dyn Widget>>,
    widths: Vec<f64>,
    row_height: f64,
    /// When `Some`, the strip is resizable: it reserves a [`GRAB_ZONE`]
    /// at each column's trailing edge, draws a separator there, and emits
    /// [`ColumnResize`] on drag. The style is the separator's colors.
    /// `None` ⇒ a plain, non-resizable strip (body/filter rows).
    separators: Option<SeparatorStyle>,
    /// Active boundary drag, if any.
    drag: Option<ColumnDrag>,
    /// Column boundary currently hovered (within [`GRAB_ZONE`]); drives
    /// separator highlight + repaint. `None` when not near a boundary.
    hovered_boundary: Option<usize>,
    /// Per-column flex weight, parallel to [`widths`](Self::widths). `0.0` =
    /// a fixed column; `> 0.0` = grows to absorb surplus width in proportion
    /// to its weight (see [`ColumnDef::flex`](super::column::ColumnDef::flex)).
    /// When every weight is `0.0` the strip never fills — it lays out at its
    /// exact `widths` (and overflows → horizontal scroll).
    flex: Vec<f64>,
    /// Surplus width to distribute across the flex columns: the allocated
    /// width minus the columns' natural total, floored at `0.0` (the portal's
    /// `content_must_fill` hands over the viewport width when the columns are
    /// narrower; otherwise they overflow → scroll). Recomputed each layout.
    /// All geometry — layout, separator paint, boundary hit-test — reads it
    /// through [`Self::col_width`] so they agree.
    fill_surplus: f64,
    /// Cached `sum(flex)`, recomputed each layout so [`Self::col_width`] can
    /// weight the surplus in O(1) instead of re-summing per column.
    flex_total: f64,
}

// --- MARK: BUILDERS
impl ColumnStrip {
    /// Creates an empty, non-resizable strip with the given row height.
    #[must_use]
    pub fn new(row_height: f64) -> Self {
        Self {
            children: Vec::new(),
            widths: Vec::new(),
            row_height,
            separators: None,
            drag: None,
            hovered_boundary: None,
            flex: Vec::new(),
            fill_surplus: 0.0,
            flex_total: 0.0,
        }
    }

    /// Sets the per-column flex weights (parallel to the widths). Extra
    /// entries are ignored and missing ones default to `0.0`, so a strip is
    /// always safe to lay out even if the weights lag the width list by a
    /// frame during a rebuild.
    #[must_use]
    pub fn with_flex(mut self, flex: Vec<f64>) -> Self {
        self.flex = flex;
        self
    }

    /// Makes the strip resizable: draws a separator with the given style
    /// at each column's trailing edge and emits [`ColumnResize`] when one
    /// is dragged.
    #[must_use]
    pub fn resizable(mut self, style: SeparatorStyle) -> Self {
        self.separators = Some(style);
        self
    }

    /// Effective on-screen width of column `i`: its base width plus its
    /// weighted share of the [`fill_surplus`](Self::fill_surplus). Layout,
    /// separator paint, and boundary hit-test all read through this so a
    /// filled strip's column pitch + trailing edges stay consistent. A
    /// fixed column (weight `0.0`) never grows; when nothing flexes the
    /// surplus is `0.0` so every column is its exact base width.
    fn col_width(&self, i: usize) -> f64 {
        let base = self.widths[i];
        let weight = self.flex.get(i).copied().unwrap_or(0.0);
        if self.fill_surplus > 0.0 && self.flex_total > 0.0 && weight > 0.0 {
            base + self.fill_surplus * weight / self.flex_total
        } else {
            base
        }
    }

    /// The strip's footprint along `axis` for a given [`LenReq`], the pure
    /// core of the [`measure`](Widget::measure) hook (which just forwards to
    /// it). Footprint is dictated entirely by the column widths (main axis)
    /// and the fixed row height (cross axis) — never the children or the
    /// layout ctx — so every strip with the same widths reports the same
    /// total width regardless of cell content, and this is unit-testable on
    /// its own.
    ///
    /// The `MinContent`/`MaxContent` vs `FitContent` split on the main axis
    /// is what keeps the header, body, and filter strips aligned:
    ///
    /// - Intrinsic width (`MinContent`/`MaxContent`) is the columns' natural
    ///   total. It drives the portal's content size and horizontal scroll
    ///   extent, so it must NOT include any fill.
    /// - `FitContent` fills to the allocated space when it's wider than the
    ///   natural total. The body's `VirtualScroll` measures each row with
    ///   `FitContent(viewport)`, so without this a body row would report
    ///   `base_total` and refuse to fill while the header (allocated the
    ///   width directly) does — the two would then misalign. Narrower space ⇒
    ///   natural total (overflow → scroll).
    fn measured_length(&self, axis: Axis, len_req: LenReq) -> Length {
        match axis {
            Axis::Horizontal => {
                let base_total: f64 = self.widths.iter().sum();
                match len_req {
                    LenReq::MinContent | LenReq::MaxContent => Length::px(base_total),
                    LenReq::FitContent(space) => Length::px(space.get().max(base_total)),
                }
            }
            Axis::Vertical => match len_req {
                LenReq::FitContent(_) | LenReq::MaxContent | LenReq::MinContent => {
                    Length::px(self.row_height)
                }
            },
        }
    }

    /// Returns the column index whose trailing boundary is within
    /// [`GRAB_ZONE`] of `x`. Every column's trailing boundary is a drag
    /// target — including the last one (the strip's outer right edge),
    /// so the final column resizes like any other.
    fn boundary_near(&self, x: f64) -> Option<usize> {
        let n = self.widths.len();
        if n == 0 {
            return None;
        }
        let mut acc = 0.0;
        for i in 0..n {
            acc += self.col_width(i);
            if (x - acc).abs() <= GRAB_ZONE {
                return Some(i);
            }
        }
        None
    }

    /// Appends a cell at the given column width (flex weight defaults to
    /// `0.0`; set the whole weight list with [`Self::with_flex`]).
    #[must_use]
    pub fn with(mut self, width: f64, child: NewWidget<impl Widget + ?Sized>) -> Self {
        self.children.push(child.erased().to_pod());
        self.widths.push(width);
        self.flex.push(0.0);
        self
    }
}

impl Default for ColumnStrip {
    fn default() -> Self {
        Self::new(0.0)
    }
}

// --- MARK: WIDGETMUT
impl ColumnStrip {
    /// Sets column `idx`'s width, requesting layout on change. Widths
    /// outside the current range are ignored.
    pub fn set_width(this: &mut WidgetMut<'_, Self>, idx: usize, width: f64) {
        if let Some(w) = this.widget.widths.get_mut(idx) {
            #[expect(clippy::float_cmp, reason = "skip layout unless the value changed")]
            if *w != width {
                *w = width;
                this.ctx.request_layout();
            }
        }
    }

    /// Sets the fixed row height, requesting layout on change.
    pub fn set_row_height(this: &mut WidgetMut<'_, Self>, row_height: f64) {
        #[expect(clippy::float_cmp, reason = "skip layout unless the value changed")]
        if this.widget.row_height != row_height {
            this.widget.row_height = row_height;
            this.ctx.request_layout();
        }
    }

    /// Sets column `idx`'s flex weight, requesting layout on change. Indices
    /// outside the current range are ignored.
    pub fn set_flex(this: &mut WidgetMut<'_, Self>, idx: usize, flex: f64) {
        if let Some(f) = this.widget.flex.get_mut(idx) {
            #[expect(clippy::float_cmp, reason = "skip layout unless the value changed")]
            if *f != flex {
                *f = flex;
                this.ctx.request_layout();
            }
        }
    }
}

// --- MARK: COLLECTIONWIDGET
impl CollectionWidget<()> for ColumnStrip {
    fn len(&self) -> usize {
        self.children.len()
    }

    fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    fn get_mut<'t>(this: &'t mut WidgetMut<'_, Self>, idx: usize) -> WidgetMut<'t, dyn Widget> {
        let child = &mut this.widget.children[idx];
        this.ctx.get_mut(child)
    }

    fn add(
        this: &mut WidgetMut<'_, Self>,
        child: NewWidget<impl Widget + ?Sized>,
        _params: impl Into<()>,
    ) {
        this.widget.children.push(child.erased().to_pod());
        // Width + flex are supplied separately by the view (via set_width /
        // set_flex); a new cell defaults to zero for both until the view
        // pushes them.
        this.widget.widths.push(0.0);
        this.widget.flex.push(0.0);
        this.ctx.children_changed();
    }

    fn insert(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: NewWidget<impl Widget + ?Sized>,
        _params: impl Into<()>,
    ) {
        this.widget.children.insert(idx, child.erased().to_pod());
        this.widget.widths.insert(idx, 0.0);
        this.widget.flex.insert(idx, 0.0);
        this.ctx.children_changed();
    }

    fn set(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: NewWidget<impl Widget + ?Sized>,
        _params: impl Into<()>,
    ) {
        let new_pod = child.erased().to_pod();
        let old_pod = std::mem::replace(&mut this.widget.children[idx], new_pod);
        this.ctx.remove_child(old_pod);
        this.ctx.children_changed();
    }

    fn set_params(this: &mut WidgetMut<'_, Self>, idx: usize, _params: impl Into<()>) {
        assert!(
            idx < this.widget.children.len(),
            "ColumnStrip::set_params: idx out of bounds"
        );
    }

    fn swap(this: &mut WidgetMut<'_, Self>, a: usize, b: usize) {
        this.widget.children.swap(a, b);
        this.widget.widths.swap(a, b);
        this.widget.flex.swap(a, b);
        this.ctx.children_changed();
    }

    fn remove(this: &mut WidgetMut<'_, Self>, idx: usize) {
        let pod = this.widget.children.remove(idx);
        this.widget.widths.remove(idx);
        this.widget.flex.remove(idx);
        this.ctx.remove_child(pod);
    }

    fn clear(this: &mut WidgetMut<'_, Self>) {
        this.widget.widths.clear();
        this.widget.flex.clear();
        for pod in this.widget.children.drain(..) {
            this.ctx.remove_child(pod);
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ColumnStrip {
    type Action = ColumnResize;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Only resizable strips react to pointer events; otherwise the
        // strip is transparent to interaction and cells handle their own.
        if self.separators.is_none() {
            return;
        }
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let x = ctx.local_position(state.position).x;
                if let Some(col) = self.boundary_near(x) {
                    self.drag = Some(ColumnDrag {
                        col,
                        base_width: self.widths[col],
                        start_x: x,
                    });
                    ctx.capture_pointer();
                    ctx.set_handled();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let x = ctx.local_position(current.position).x;
                if let Some(drag) = self.drag {
                    if ctx.is_active() {
                        let new_width = drag.base_width + (x - drag.start_x);
                        ctx.submit_action::<Self::Action>(ColumnResize {
                            col: drag.col,
                            new_width,
                        });
                    }
                } else {
                    // Track hover for separator highlight + cursor.
                    let near = self.boundary_near(x);
                    if near != self.hovered_boundary {
                        self.hovered_boundary = near;
                        ctx.request_paint_only();
                    }
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) if self.drag.take().is_some() => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Clear hover highlight when the pointer leaves the strip.
        if let Update::HoveredChanged(false) = event
            && self.hovered_boundary.take().is_some()
        {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in &mut self.children {
            ctx.register_child(child);
        }
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        // `pos` is in window coordinates — convert to this widget's local
        // space before boundary-testing (matches masonry's `Split`).
        let local_x = ctx.to_local(pos).x;
        if self.separators.is_some()
            && (self.drag.is_some() || self.boundary_near(local_x).is_some())
        {
            CursorIcon::EwResize
        } else {
            CursorIcon::Default
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The footprint depends only on the strip's own widths/row height and
        // the request — never on a child or the ctx — so the logic lives in a
        // pure helper that can be unit-tested without a `MeasureCtx`.
        self.measured_length(axis, len_req)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // Authoritative placement: force each child to exactly its size
        // and place it at the cumulative x. The parent chooses the child
        // size outright (per `run_layout` semantics), so cell content can
        // never widen a column — this is what guarantees alignment across
        // header/body/filter strips.
        //
        // Every cell is laid out `GRAB_ZONE` narrower than its column
        // (see `cell_layout_width`). On a resizable strip that keeps the
        // trailing grab strip clear of children: masonry's hit-test only
        // consults a widget's `get_cursor` when it (not a child) is the
        // hovered widget, so leaving the zone bare lets the strip own
        // both the resize cursor *and* the drag hit-test — exactly how
        // masonry's `Split` keeps its bar clear. Applying the same inset
        // to non-resizable strips keeps End-aligned header/body/filter
        // content at identical x-positions. Column *pitch* is unchanged.
        //
        // #111 fill: distribute any surplus (allocated width beyond the
        // columns' natural total, handed over by the portal's
        // `content_must_fill` when the viewport is wider) across the flex
        // columns by weight — see `col_width`. When no column flexes the
        // surplus goes unused and every column is its exact base width, so
        // the strip overflows and scrolls exactly as before. Alignment across
        // header/body/filter holds because every strip gets the same
        // allocated width and the same weights, so all compute the same
        // distribution.
        let base_total: f64 = self.widths.iter().sum();
        self.fill_surplus = (size.width - base_total).max(0.0);
        self.flex_total = self.flex.iter().sum();
        let mut x = 0.0;
        let height = self.row_height;
        for i in 0..self.children.len() {
            let w = self.col_width(i);
            let child = &mut self.children[i];
            ctx.run_layout(child, Size::new(cell_layout_width(w), height));
            ctx.place_child(child, Point::new(x, 0.0));
            x += w;
        }
        ctx.clear_baselines();
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        // Resizable strips draw a 1px separator at each column boundary,
        // brightened for the hovered/dragged boundary.
        let Some(style) = self.separators else {
            return;
        };
        let height = ctx.border_box_size().height;
        let active = self.drag.map(|d| d.col).or(self.hovered_boundary);
        // Running accumulator (like `layout`) so painting n separators is
        // O(n), not O(n²) via a fresh prefix-sum per boundary. Reads
        // `col_width` so the last column's separator tracks the fill surplus.
        let mut x = 0.0;
        for i in 0..self.widths.len() {
            x += self.col_width(i);
            let color = if active == Some(i) {
                style.active
            } else {
                style.line
            };
            if color.components[3] <= 0.0 {
                continue;
            }
            // 1px line just inside the boundary so it isn't clipped.
            let rect = Rect::from_origin_size(Point::new(x - 1.0, 0.0), Size::new(1.0, height));
            painter.fill(rect, color).draw();
        }
    }

    fn accepts_pointer_interaction(&self) -> bool {
        // Resizable strips must receive pointer events (to hit-test
        // boundaries); plain strips stay transparent so their cells get
        // their own clicks/hover.
        self.separators.is_some()
    }

    fn accessibility_role(&self) -> Role {
        Role::Row
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.children.iter().map(WidgetPod::id).collect();
        ChildrenIds::from_slice(&ids)
    }
}

// === MARK: VIEW ========================================================

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::core::FromDynWidget;
use xilem::core::{
    AppendVec, ElementSplice, MessageCtx, MessageResult, Mut, SuperElement, View, ViewElement,
    ViewMarker, ViewSequence,
};
use xilem::{Pod, ViewCtx};

/// A column-strip view. `widths` is the per-column width list; `cells` is
/// a view sequence producing one cell per width (same length and order).
/// All cells are placed at authoritative x-positions, so multiple strips
/// sharing a width list are pixel-aligned. Construct via [`column_strip`].
/// Boxed resize callback: `(state, column, new_width)`.
type ResizeCb<State, Action> = Box<dyn Fn(&mut State, usize, f64) -> Action + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ColumnStripView<Seq, State, Action = ()> {
    /// Shared per-column widths. `Arc` so the grid body can hand the
    /// same width list to every visible row as a refcount bump instead
    /// of a fresh `Vec` allocation per row per rebuild.
    widths: Arc<Vec<f64>>,
    /// Per-column flex weights, parallel to `widths`. Empty ⇒ every column
    /// is fixed (no fill). Shared via `Arc` so the body hands the same list
    /// to every visible row as a refcount bump, like `widths`.
    flex: Arc<Vec<f64>>,
    row_height: f64,
    cells: Seq,
    /// Resize config: separator style + on-resize callback. `None` ⇒ a
    /// plain, non-resizable strip.
    resize: Option<(SeparatorStyle, ResizeCb<State, Action>)>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// Creates a [`ColumnStripView`] from a width list, a fixed row height,
/// and a sequence of cell views (one per width, in order). `widths`
/// takes a plain `Vec` or a shared `Arc<Vec<_>>` — pass the `Arc` when
/// many strips (e.g. every body row) share one width list.
pub fn column_strip<Seq, State, Action>(
    widths: impl Into<Arc<Vec<f64>>>,
    row_height: f64,
    cells: Seq,
) -> ColumnStripView<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: ColumnStripSequence<State, Action>,
{
    ColumnStripView {
        widths: widths.into(),
        flex: Arc::new(Vec::new()),
        row_height,
        cells,
        resize: None,
        phantom: PhantomData,
    }
}

impl<Seq, State, Action> ColumnStripView<Seq, State, Action> {
    /// Supplies per-column flex weights (parallel to the width list) for
    /// viewport-fill sizing. A column with weight `> 0.0` grows to absorb
    /// surplus width in proportion to its weight; the default (no weights, or
    /// all `0.0`) is fixed columns with horizontal scroll. Pass the *same*
    /// `Arc` to the header, filter, and body strips so they distribute
    /// identically and stay aligned.
    pub fn flex_weights(mut self, flex: impl Into<Arc<Vec<f64>>>) -> Self {
        self.flex = flex.into();
        self
    }

    /// Makes the strip resizable: it draws a separator (with `style`) at
    /// each column's trailing edge and calls `on_resize(state, col,
    /// new_width)` while a boundary is dragged. The strip owns the grab
    /// zones itself (no overlay), so cell content keeps its own hover and
    /// clicks.
    pub fn resizable<F>(mut self, style: SeparatorStyle, on_resize: F) -> Self
    where
        F: Fn(&mut State, usize, f64) -> Action + Send + Sync + 'static,
    {
        self.resize = Some((style, Box::new(on_resize)));
        self
    }
}

mod hidden {
    use xilem::core::AppendVec;

    use super::ColumnStripElement;

    #[doc(hidden)]
    #[expect(
        unnameable_types,
        reason = "implementation detail; public solely because of trait visibility rules"
    )]
    pub struct ColumnStripState<SeqState> {
        pub(crate) seq_state: SeqState,
        pub(crate) scratch: AppendVec<ColumnStripElement>,
    }
}

use hidden::ColumnStripState;

impl<Seq, State, Action> ViewMarker for ColumnStripView<Seq, State, Action> {}

impl<State, Action, Seq> View<State, Action, ViewCtx> for ColumnStripView<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: ColumnStripSequence<State, Action>,
{
    type Element = Pod<ColumnStrip>;
    type ViewState = ColumnStripState<Seq::SeqState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut elements = AppendVec::default();
        let mut w = ColumnStrip::new(self.row_height);
        if let Some((style, _)) = &self.resize {
            w = w.resizable(*style);
        }
        let seq_state = self.cells.seq_build(ctx, &mut elements, app_state);
        for (width, element) in self.widths.iter().copied().zip(elements.drain()) {
            w = w.with(width, element.child.new_widget);
        }
        // Apply flex weights, matched to the width-list length (fixed = all
        // zero when unset or mismatched) so the widget is always safe to lay
        // out even if the weights lag the widths by a frame.
        let n = self.widths.len();
        let flex_vec = if self.flex.len() == n {
            self.flex.as_ref().clone()
        } else {
            vec![0.0; n]
        };
        w = w.with_flex(flex_vec);
        // Register as an action source when resizable so the widget's
        // `ColumnResize` actions route to this view's `message`.
        let pod = if self.resize.is_some() {
            ctx.with_action_widget(|ctx| ctx.create_pod(w))
        } else {
            ctx.create_pod(w)
        };
        (
            pod,
            ColumnStripState {
                seq_state,
                scratch: elements,
            },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        ColumnStripState { seq_state, scratch }: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        #[expect(clippy::float_cmp, reason = "skip the setter unless bit-identical")]
        if self.row_height != prev.row_height {
            ColumnStrip::set_row_height(&mut element, self.row_height);
        }
        // Splice the cell views FIRST so the widget's child set matches
        // `self.widths` before we push widths. A new cell is inserted with
        // a default width of 0 (see `CollectionWidget::add`), so widths
        // must be applied *after* the splice — otherwise a freshly shown
        // column stays 0-wide (invisible). Scoped in a block so the
        // splice's `WidgetMut` is released before the width sync.
        {
            let mut splice = ColumnStripSplice::new(element.reborrow_mut(), scratch);
            self.cells
                .seq_rebuild(&prev.cells, seq_state, ctx, &mut splice, app_state);
            debug_assert!(scratch.is_empty());
        }
        // Now sync every column width against the settled child set.
        // Unconditional (not diffed against `prev.widths` positionally):
        // a reorder/insert/delete shifts which column sits at each index,
        // so a positional diff would miss real changes. `set_width` skips
        // the layout request when the value is already current, so this is
        // cheap when nothing moved.
        for (idx, &w) in self.widths.iter().enumerate() {
            ColumnStrip::set_width(&mut element, idx, w);
        }
        // Sync flex weights against the settled child set (same rationale as
        // widths above). No-op when unset, so non-flex strips are untouched.
        for (idx, &f) in self.flex.iter().enumerate() {
            ColumnStrip::set_flex(&mut element, idx, f);
        }
    }

    fn teardown(
        &self,
        ColumnStripState { seq_state, scratch }: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        if self.resize.is_some() {
            ctx.teardown_action_source(element.reborrow_mut());
        }
        let mut splice = ColumnStripSplice::new(element, scratch);
        self.cells.seq_teardown(seq_state, ctx, &mut splice);
        debug_assert!(scratch.is_empty());
    }

    fn message(
        &self,
        ColumnStripState { seq_state, scratch }: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // An empty remaining path means the message reached *this* view
        // as its terminal target — i.e. the strip widget's own
        // `ColumnResize` action. A non-empty path means it's bound for a
        // child cell, so delegate to the sequence. Calling `take_message`
        // with a non-empty path panics ("not reached its target"), so the
        // path check must gate it.
        if message.remaining_path().is_empty() {
            if let Some((_, on_resize)) = &self.resize
                && let Some(resize) = message.take_message::<ColumnResize>()
            {
                // Return the callback's Action (not `Nop`/`RequestRebuild`)
                // so the host's app_logic re-runs and the updated column
                // widths flow back into every strip.
                return MessageResult::Action(on_resize(app_state, resize.col, resize.new_width));
            }
            return MessageResult::Nop;
        }
        let mut splice = ColumnStripSplice::new(element, scratch);
        let result = self
            .cells
            .seq_message(seq_state, message, &mut splice, app_state);
        debug_assert!(scratch.is_empty());
        result
    }
}

// === MARK: ELEMENT =====================================================

/// A cell slot in a [`ColumnStrip`] view — a thin wrapper around an
/// erased pod (no per-cell params; width is tracked by the strip widget).
pub struct ColumnStripElement {
    child: Pod<dyn Widget>,
}

/// Mutable-reference form of [`ColumnStripElement`], used by xilem's
/// `Mut` machinery.
pub struct ColumnStripElementMut<'w> {
    parent: WidgetMut<'w, ColumnStrip>,
    idx: usize,
}

impl ViewElement for ColumnStripElement {
    type Mut<'w> = ColumnStripElementMut<'w>;
}

impl SuperElement<Self, ViewCtx> for ColumnStripElement {
    fn upcast(_ctx: &mut ViewCtx, child: Self) -> Self {
        child
    }

    fn with_downcast_val<R>(
        mut this: Mut<'_, Self>,
        f: impl FnOnce(Mut<'_, Self>) -> R,
    ) -> (Self::Mut<'_>, R) {
        let r = {
            let parent = this.parent.reborrow_mut();
            let reborrow = ColumnStripElementMut {
                idx: this.idx,
                parent,
            };
            f(reborrow)
        };
        (this, r)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> SuperElement<Pod<W>, ViewCtx> for ColumnStripElement {
    fn upcast(_: &mut ViewCtx, child: Pod<W>) -> Self {
        Self {
            child: child.erased(),
        }
    }

    fn with_downcast_val<R>(
        mut this: Mut<'_, Self>,
        f: impl FnOnce(Mut<'_, Pod<W>>) -> R,
    ) -> (Mut<'_, Self>, R) {
        let ret = {
            let mut child = ColumnStrip::get_mut(&mut this.parent, this.idx);
            let downcast = child.downcast();
            f(downcast)
        };
        (this, ret)
    }
}

// === MARK: SPLICE ======================================================

/// `ElementSplice` for a live [`ColumnStrip`]. Walks an [`AppendVec`] of
/// scratch elements plus an index into the children, translating to
/// `CollectionWidget` calls on the masonry widget.
struct ColumnStripSplice<'w, 's> {
    idx: usize,
    element: WidgetMut<'w, ColumnStrip>,
    scratch: &'s mut AppendVec<ColumnStripElement>,
}

impl<'w, 's> ColumnStripSplice<'w, 's> {
    fn new(
        element: WidgetMut<'w, ColumnStrip>,
        scratch: &'s mut AppendVec<ColumnStripElement>,
    ) -> Self {
        Self {
            idx: 0,
            element,
            scratch,
        }
    }
}

impl ElementSplice<ColumnStripElement> for ColumnStripSplice<'_, '_> {
    fn with_scratch<R>(&mut self, f: impl FnOnce(&mut AppendVec<ColumnStripElement>) -> R) -> R {
        let ret = f(self.scratch);
        for element in self.scratch.drain() {
            ColumnStrip::insert(&mut self.element, self.idx, element.child.new_widget, ());
            self.idx += 1;
        }
        ret
    }

    fn insert(&mut self, element: ColumnStripElement) {
        ColumnStrip::insert(&mut self.element, self.idx, element.child.new_widget, ());
        self.idx += 1;
    }

    fn mutate<R>(&mut self, f: impl FnOnce(Mut<'_, ColumnStripElement>) -> R) -> R {
        let child = ColumnStripElementMut {
            parent: self.element.reborrow_mut(),
            idx: self.idx,
        };
        let ret = f(child);
        self.idx += 1;
        ret
    }

    fn skip(&mut self, n: usize) {
        self.idx += n;
    }

    fn index(&self) -> usize {
        self.idx
    }

    fn delete<R>(&mut self, f: impl FnOnce(Mut<'_, ColumnStripElement>) -> R) -> R {
        let ret = {
            let child = ColumnStripElementMut {
                parent: self.element.reborrow_mut(),
                idx: self.idx,
            };
            f(child)
        };
        ColumnStrip::remove(&mut self.element, self.idx);
        ret
    }
}

// === MARK: SEQUENCE TRAIT =============================================

/// Marker trait for any [`ViewSequence`] producing [`ColumnStripElement`]s.
/// Blanket-implemented, so tuples of mixed cell views compose naturally —
/// like xilem's stock `FlexSequence` / this crate's `FlexWrapSequence`.
pub trait ColumnStripSequence<State: 'static, Action = ()>:
    ViewSequence<State, Action, ViewCtx, ColumnStripElement>
{
}

impl<Seq, State, Action> ColumnStripSequence<State, Action> for Seq
where
    Seq: ViewSequence<State, Action, ViewCtx, ColumnStripElement>,
    State: 'static,
{
}

// === MARK: TESTS =======================================================

#[cfg(test)]
mod tests {
    use super::{Axis, ColumnStrip, GRAB_ZONE, LenReq, Length, cell_layout_width};

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// A bare strip with the given widths — `boundary_near` reads only
    /// `widths`, so no children/context are needed.
    fn strip_with_widths(widths: Vec<f64>) -> ColumnStrip {
        ColumnStrip {
            children: Vec::new(),
            widths,
            row_height: 20.0,
            separators: None,
            drag: None,
            hovered_boundary: None,
            flex: Vec::new(),
            fill_surplus: 0.0,
            flex_total: 0.0,
        }
    }

    #[test]
    fn cell_layout_width_reserves_the_grab_zone() {
        // Every strip (resizable or not) insets cells by GRAB_ZONE so
        // End-aligned header/body/filter content lines up by construction.
        assert!(approx(cell_layout_width(100.0), 100.0 - GRAB_ZONE));
    }

    #[test]
    fn cell_layout_width_floors_at_zero() {
        // A column narrower than the grab zone yields a zero-width (not
        // negative) cell; ColumnWidths::effective clamps grid columns to
        // MIN_COLUMN_WIDTH well above this, so only direct strip users
        // can get here.
        assert!(approx(cell_layout_width(GRAB_ZONE / 2.0), 0.0));
    }

    #[test]
    fn col_width_distributes_surplus_by_flex_weight() {
        // Two flex columns (weights 1 and 3) share 80px of surplus 1:3; the
        // fixed column (weight 0) is unchanged. Layout caches `flex_total`, so
        // set it to match the weights.
        let mut strip = strip_with_widths(vec![100.0, 100.0, 100.0]);
        strip.flex = vec![0.0, 1.0, 3.0];
        strip.flex_total = 4.0;
        strip.fill_surplus = 80.0;
        assert!(approx(strip.col_width(0), 100.0), "fixed column unchanged");
        assert!(approx(strip.col_width(1), 100.0 + 20.0), "weight 1 → +20");
        assert!(approx(strip.col_width(2), 100.0 + 60.0), "weight 3 → +60");
        // The distributed surplus exactly fills the allocation.
        let total: f64 = (0..3).map(|i| strip.col_width(i)).sum();
        assert!(approx(total, 300.0 + 80.0));
    }

    #[test]
    fn col_width_is_base_when_nothing_flexes() {
        // Surplus present but no flex weight → it goes unused and every column
        // stays its base width (the grid overflows and scrolls instead).
        let mut strip = strip_with_widths(vec![100.0, 120.0]);
        strip.flex = vec![0.0, 0.0];
        strip.flex_total = 0.0;
        strip.fill_surplus = 50.0;
        assert!(approx(strip.col_width(0), 100.0));
        assert!(approx(strip.col_width(1), 120.0));
    }

    #[test]
    fn measured_width_intrinsic_is_the_columns_natural_total() {
        // MinContent/MaxContent must report the columns' natural total and
        // never any fill — this is the portal's content size + horizontal
        // scroll extent. Fill here would break scrolling on a narrow viewport.
        let strip = strip_with_widths(vec![80.0, 100.0, 60.0]);
        assert!(approx(
            strip
                .measured_length(Axis::Horizontal, LenReq::MinContent)
                .get(),
            240.0,
        ));
        assert!(approx(
            strip
                .measured_length(Axis::Horizontal, LenReq::MaxContent)
                .get(),
            240.0,
        ));
    }

    #[test]
    fn measured_width_fits_content_fills_wider_allocation() {
        // FitContent wider than the natural total fills to the allocation, so
        // a body row (measured `FitContent(viewport)`) lines up with the
        // header (allocated the width directly) instead of stopping short.
        let strip = strip_with_widths(vec![80.0, 100.0, 60.0]);
        assert!(approx(
            strip
                .measured_length(Axis::Horizontal, LenReq::FitContent(Length::px(400.0)))
                .get(),
            400.0,
        ));
    }

    #[test]
    fn measured_width_fits_content_floors_at_the_natural_total() {
        // A narrower allocation ⇒ natural total, so the strip overflows and
        // scrolls rather than crushing the columns below their widths.
        let strip = strip_with_widths(vec![80.0, 100.0, 60.0]);
        assert!(approx(
            strip
                .measured_length(Axis::Horizontal, LenReq::FitContent(Length::px(120.0)))
                .get(),
            240.0,
        ));
    }

    #[test]
    fn measured_height_is_the_fixed_row_height_for_every_request() {
        // The cross axis is the fixed row height regardless of the request —
        // cell content can never grow (or shrink) a strip's row.
        let strip = strip_with_widths(vec![80.0, 100.0]);
        for len_req in [
            LenReq::MinContent,
            LenReq::MaxContent,
            LenReq::FitContent(Length::px(500.0)),
        ] {
            assert!(approx(
                strip.measured_length(Axis::Vertical, len_req).get(),
                20.0,
            ));
        }
    }

    #[test]
    fn boundary_near_hits_every_trailing_boundary_including_the_last() {
        let strip = strip_with_widths(vec![80.0, 100.0]);
        // Interior boundary.
        assert_eq!(strip.boundary_near(80.0), Some(0));
        // The strip's outer right edge is the last column's trailing
        // boundary — deliberately draggable, so the last column resizes
        // like any other.
        assert_eq!(strip.boundary_near(180.0), Some(1));
        // Within tolerance on either side.
        assert_eq!(strip.boundary_near(80.0 - GRAB_ZONE), Some(0));
        assert_eq!(strip.boundary_near(180.0 + GRAB_ZONE), Some(1));
        // Mid-column is no boundary.
        assert_eq!(strip.boundary_near(130.0), None);
    }
}
