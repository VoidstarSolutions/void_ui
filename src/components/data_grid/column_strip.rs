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
    MeasureCtx, NewWidget, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

/// Width (px) of the parent-owned grab zone at each column's trailing
/// edge when the strip is resizable. Reserved from the cell so no child
/// covers it — the strip itself receives pointer events there, the way
/// masonry's `Split` reserves its bar region. Also the hover tolerance.
const GRAB_ZONE: f64 = 6.0;

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
        }
    }

    /// Makes the strip resizable: draws a separator with the given style
    /// at each column's trailing edge and emits [`ColumnResize`] when one
    /// is dragged.
    #[must_use]
    pub fn resizable(mut self, style: SeparatorStyle) -> Self {
        self.separators = Some(style);
        self
    }

    /// Cumulative x of column `i`'s trailing boundary (sum of widths
    /// `0..=i`).
    fn boundary_x(&self, i: usize) -> f64 {
        self.widths[..=i].iter().sum()
    }

    /// Returns the column index whose trailing boundary is within
    /// [`GRAB_ZONE`] of `x` (excluding the very last boundary, which is
    /// the strip's outer edge — not a resize target).
    fn boundary_near(&self, x: f64) -> Option<usize> {
        let n = self.widths.len();
        if n == 0 {
            return None;
        }
        // Only interior + non-final trailing boundaries resize a column;
        // every column 0..n has a trailing boundary, all draggable.
        let mut acc = 0.0;
        for (i, &w) in self.widths.iter().enumerate() {
            acc += w;
            if (x - acc).abs() <= GRAB_ZONE {
                return Some(i);
            }
        }
        None
    }

    /// Appends a cell at the given column width.
    #[must_use]
    pub fn with(mut self, width: f64, child: NewWidget<impl Widget + ?Sized>) -> Self {
        self.children.push(child.erased().to_pod());
        self.widths.push(width);
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
        // Width is supplied separately by the view (via set_width); a new
        // cell defaults to zero until the view pushes its width.
        this.widget.widths.push(0.0);
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
        this.ctx.children_changed();
    }

    fn remove(this: &mut WidgetMut<'_, Self>, idx: usize) {
        let pod = this.widget.children.remove(idx);
        this.widget.widths.remove(idx);
        this.ctx.remove_child(pod);
    }

    fn clear(this: &mut WidgetMut<'_, Self>) {
        this.widget.widths.clear();
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
        if self.separators.is_some() && (self.drag.is_some() || self.boundary_near(local_x).is_some())
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
        // Footprint is dictated entirely by the column widths (main axis)
        // and the fixed row height (cross axis) — never the children. So
        // every strip with the same widths reports the same total width,
        // regardless of cell content.
        match axis {
            Axis::Horizontal => Length::px(self.widths.iter().sum()),
            Axis::Vertical => match len_req {
                LenReq::FitContent(_) | LenReq::MaxContent | LenReq::MinContent => {
                    Length::px(self.row_height)
                }
            },
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        // Authoritative placement: force each child to exactly its size
        // and place it at the cumulative x. The parent chooses the child
        // size outright (per `run_layout` semantics), so cell content can
        // never widen a column — this is what guarantees alignment across
        // header/body/filter strips.
        //
        // When resizable, each cell is laid out `GRAB_ZONE` narrower than
        // its column so the trailing grab strip is NOT covered by a
        // child. Masonry's hit-test only consults a widget's `get_cursor`
        // when it (not a child) is the hovered widget; leaving the grab
        // strip bare lets the strip own both the resize cursor *and* the
        // drag hit-test there — exactly how masonry's `Split` keeps its
        // bar clear of its children. Column *pitch* is unchanged, so
        // alignment with the (full-width) body/filter strips holds.
        let resizable = self.separators.is_some();
        let mut x = 0.0;
        let height = self.row_height;
        for (child, &width) in self.children.iter_mut().zip(self.widths.iter()) {
            let cell_w = if resizable {
                (width - GRAB_ZONE).max(0.0)
            } else {
                width
            };
            ctx.run_layout(child, Size::new(cell_w, height));
            ctx.place_child(child, Point::new(x, 0.0));
            x += width;
        }
        ctx.clear_baselines();
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        // Resizable strips draw a 1px separator at each column boundary,
        // brightened for the hovered/dragged boundary.
        let Some(style) = self.separators else {
            return;
        };
        let height = ctx.border_box_size().height;
        let active = self.drag.map(|d| d.col).or(self.hovered_boundary);
        for i in 0..self.widths.len() {
            let x = self.boundary_x(i);
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
type ResizeCb<State> = Box<dyn Fn(&mut State, usize, f64) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ColumnStripView<Seq, State, Action = ()> {
    widths: Vec<f64>,
    row_height: f64,
    cells: Seq,
    /// Resize config: separator style + on-resize callback. `None` ⇒ a
    /// plain, non-resizable strip.
    resize: Option<(SeparatorStyle, ResizeCb<State>)>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// Creates a [`ColumnStripView`] from a width list, a fixed row height,
/// and a sequence of cell views (one per width, in order).
pub fn column_strip<Seq, State, Action>(
    widths: Vec<f64>,
    row_height: f64,
    cells: Seq,
) -> ColumnStripView<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: ColumnStripSequence<State, Action>,
{
    ColumnStripView {
        widths,
        row_height,
        cells,
        resize: None,
        phantom: PhantomData,
    }
}

impl<Seq, State, Action> ColumnStripView<Seq, State, Action> {
    /// Makes the strip resizable: it draws a separator (with `style`) at
    /// each column's trailing edge and calls `on_resize(state, col,
    /// new_width)` while a boundary is dragged. The strip owns the grab
    /// zones itself (no overlay), so cell content keeps its own hover and
    /// clicks.
    pub fn resizable<F>(mut self, style: SeparatorStyle, on_resize: F) -> Self
    where
        F: Fn(&mut State, usize, f64) + Send + Sync + 'static,
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
    // `Default` lets a resize emit `MessageResult::Action(Action::default())`,
    // which re-runs the host's app_logic so the new column widths flow
    // back in. (`()` — the grid's action type — implements `Default`.)
    Action: 'static + Default,
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
        // Push per-column width changes (the cell views themselves are
        // diffed by the sequence splice below).
        for (idx, &w) in self.widths.iter().enumerate() {
            let changed = prev.widths.get(idx).is_none_or(|&pw| {
                #[expect(clippy::float_cmp, reason = "only update changed widths")]
                let neq = pw != w;
                neq
            });
            if changed {
                ColumnStrip::set_width(&mut element, idx, w);
            }
        }
        let mut splice = ColumnStripSplice::new(element, scratch);
        self.cells
            .seq_rebuild(&prev.cells, seq_state, ctx, &mut splice, app_state);
        debug_assert!(scratch.is_empty());
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
                on_resize(app_state, resize.col, resize.new_width);
                // Return `Action` (not `Nop`/`RequestRebuild`) so the
                // host's app_logic re-runs and the updated column widths
                // flow back into every strip. `RequestRebuild` only
                // re-evaluates the *existing* view values against
                // themselves (widths unchanged there) → "tree didn't
                // change"; an Action propagating to the root is what
                // actually re-renders, exactly like Slider/Button.
                return MessageResult::Action(Action::default());
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
