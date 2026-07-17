//! Generic N-cell grid masonry widget for the date picker.
//!
//! [`CalendarGridWidget`] renders an arbitrary flat list of cells in a fixed
//! column grid. Each cell is a [`CalendarCell`] child pod, which wraps a
//! `Label` and exposes it to accessibility as `Role::GridCell` with the
//! cell's `selected`/`disabled` state (mirrors `TabItemNode` in
//! `components::tabs::widget`). Backgrounds (selection fill, hover fill,
//! today dot, focus ring) are drawn directly by the parent `paint` pass, and
//! the parent also owns all interaction — pointer hit-testing, hover, click,
//! and roving keyboard focus — the cell pod is a passive accessibility/visual
//! wrapper. The widget does not own calendar logic — it only knows about
//! cells, colors, and interaction state.
//!
//! Used by `CalendarBodyWidget` for day grids (7 columns × 6 rows), month
//! grids (3 × 4), and year grids (4 × 5).

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
    PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Insets, Point, Rect, RoundedRect, Size, Vec2};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::collection::single_child;
use crate::components::click::{ClickPhase, primary_click_when};
use crate::components::item_list::{hit_item, index_f64, to_local};
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};

// ─────────────────────────────────────────────────────────────────────────────
// Today-dot geometry — fixed chrome, not density-scaled.
// ─────────────────────────────────────────────────────────────────────────────

/// Radius of the "today" indicator dot at the bottom of a day cell.
const TODAY_DOT_RADIUS: f64 = 2.5;
/// Distance from the bottom edge of the cell to the center of the today dot.
const TODAY_DOT_MARGIN_B: f64 = 4.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Per-cell metadata supplied by [`CalendarBodyWidget`].
#[derive(Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent cell flags, no state machine"
)]
pub(crate) struct CellDatum {
    /// Display text — the day number, month abbreviation, or year string.
    pub label: ArcStr,
    /// Whether this cell is the currently selected value.
    pub selected: bool,
    /// Day mode only: paint a small accent dot beneath this cell.
    pub today: bool,
    /// Cell is outside the permitted `[min, max]` date range.
    pub disabled: bool,
    /// Day mode: date belongs to a neighboring month (leading / trailing filler).
    pub muted: bool,
}

/// Action emitted by [`CalendarGridWidget`] when the user activates a cell.
#[derive(Debug)]
pub(crate) enum CalendarGridAction {
    /// The user clicked or pressed Enter on the given cell index.
    CellActivated(usize),
}

/// Wraps a cell's `Label` content, exposing it to accessibility as
/// `Role::GridCell` with the cell's `selected`/`disabled` state.
///
/// [`CalendarGridWidget`] still owns all interaction (pointer hit-testing,
/// hover, click, roving keyboard focus) and painting (selection fill, focus
/// ring, today dot) — this wrapper only makes that already-existing state
/// legible to assistive tech. It never accepts pointer input or real focus
/// itself, so it can't intercept anything the parent already handles.
/// Mirrors `TabItemNode` (`components::tabs::widget`), the same
/// composite-widget pattern for a single-focus-stop, roving-selection
/// control.
struct CalendarCell {
    child: WidgetPod<Label>,
    selected: bool,
    disabled: bool,
}

impl CalendarCell {
    fn new(datum: &CellDatum, theme: &Theme) -> Self {
        let mut lbl = Label::new(datum.label.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties
            .insert(ContentColor::new(text_color_for(datum, theme)));
        Self {
            child: lbl.to_pod(),
            selected: datum.selected,
            disabled: datum.disabled,
        }
    }

    /// Mutable access to the wrapped `Label`, for [`CalendarGridWidget::set_theme`].
    fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, Label> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for CalendarCell {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // `new` only sets the widget-internal field; without this the first
        // build leaves masonry's `is_disabled()` out of sync, and the
        // accessibility pass's own `node.set_disabled()` (driven by
        // `ctx.is_disabled()`) would never fire.
        if matches!(event, Update::WidgetAdded) {
            ctx.set_disabled(self.disabled);
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        single_child::measure(ctx, &mut self.child, axis, len_req, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        single_child::layout(ctx, &mut self.child, size);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GridCell
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_selected(self.selected);
        if !self.disabled {
            node.add_action(masonry::accesskit::Action::Click);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        single_child::children_ids(&self.child)
    }
}

/// Generic N-cell grid masonry widget.
///
/// Renders a flat list of `N` cells in a fixed `cols`-column grid. The number
/// of rows is `ceil(N / cols)`. All cells are square; the side length is
/// `theme.density.row_height + theme.density.pad_v`.
pub(crate) struct CalendarGridWidget {
    /// One [`CalendarCell`] pod per cell — wraps the cell's `Label` and
    /// exposes accessibility state (see [`CalendarCell`]).
    cells: Vec<WidgetPod<CalendarCell>>,
    /// Parallel metadata for each cell.
    data: Vec<CellDatum>,
    /// Number of columns in the grid.
    cols: usize,
    /// Cell bounding rects in widget-local space, populated by `layout`.
    cell_rects: Vec<Rect>,
    /// Index of the cell under the pointer, if any.
    hover_index: Option<usize>,
    /// Keyboard-roving focus index, driven externally by `CalendarBodyWidget`.
    focused_index: Option<usize>,
    /// Theme snapshot — copied in on construction and updated by `set_theme`.
    theme: Theme,
}

// ─────────────────────────────────────────────────────────────────────────────
// Constructor helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the text color that a cell label should use given its datum and
/// the current theme.
fn text_color_for(datum: &CellDatum, theme: &Theme) -> masonry::peniko::Color {
    let p = &theme.palette;
    if datum.disabled {
        p.text_faint
    } else if datum.selected {
        // Selected cells get a filled accent background — use bg for contrast.
        // TODO: replace with palette.on_accent once that token is added to Palette
        p.bg
    } else if datum.muted {
        p.text_muted
    } else {
        p.text
    }
}

/// Builds a single [`CalendarCell`] pod for `datum`, styled with `theme`.
fn make_cell(datum: &CellDatum, theme: &Theme) -> WidgetPod<CalendarCell> {
    WidgetPod::new(CalendarCell::new(datum, theme))
}

/// Computes the side length (width = height) of a calendar cell from density
/// tokens. Exported as a free function so `CalendarBodyWidget` can compute the
/// total grid size without holding a reference to the widget.
pub(crate) fn cell_side(theme: &Theme) -> f64 {
    f64::from(theme.density.row_height) + f64::from(theme.density.pad_v)
}

// ─────────────────────────────────────────────────────────────────────────────
// Impl block — constructor + WidgetMut setters
// ─────────────────────────────────────────────────────────────────────────────

impl CalendarGridWidget {
    /// Creates a new grid widget with one [`CalendarCell`] pod per datum.
    pub(crate) fn new(data: Vec<CellDatum>, cols: usize, theme: &Theme) -> Self {
        let cells = data.iter().map(|d| make_cell(d, theme)).collect();
        Self {
            cells,
            data,
            cols,
            cell_rects: Vec::new(),
            hover_index: None,
            focused_index: None,
            theme: *theme,
        }
    }

    // ── MARK: WidgetMut setters ───────────────────────────────────────────────

    /// Replaces the cell data and column count, reconciling pods (add / remove
    /// as needed), and requests a layout + repaint.
    ///
    /// `cols` must match the grid layout for the new data set (7 for Day mode,
    /// 4 for Month/Year mode). Failing to update it would keep the old column
    /// count and produce garbled layout when switching view modes.
    pub(crate) fn set_data(this: &mut WidgetMut<'_, Self>, data: Vec<CellDatum>, cols: usize) {
        // Drain all existing pods.
        for pod in this.widget.cells.drain(..) {
            this.ctx.remove_child(pod);
        }
        let theme = this.widget.theme;
        // Build fresh pods from the new data.
        this.widget.cells = data.iter().map(|d| make_cell(d, &theme)).collect();
        this.widget.data = data;
        this.widget.cols = cols;
        this.widget.cell_rects.clear();
        this.widget.hover_index = None;
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Updates the keyboard-roving focus index and requests a repaint.
    pub(crate) fn set_focused_index(this: &mut WidgetMut<'_, Self>, index: Option<usize>) {
        if this.widget.focused_index != index {
            this.widget.focused_index = index;
            this.ctx.request_paint_only();
        }
    }

    /// Applies a new theme: updates the stored theme, pushes new colors to all
    /// child label pods, and requests a layout + repaint.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        // Push updated colors to each cell's label.
        for (pod, datum) in this.widget.cells.iter_mut().zip(this.widget.data.iter()) {
            let color = text_color_for(datum, theme);
            let mut cell = this.ctx.get_mut(pod);
            let mut lbl = CalendarCell::child_mut(&mut cell);
            lbl.insert_prop(ContentColor::new(color));
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(theme.density.ui_font_size),
            );
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Widget trait
// ─────────────────────────────────────────────────────────────────────────────

impl Widget for CalendarGridWidget {
    type Action = CalendarGridAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for cell in &mut self.cells {
            ctx.register_child(cell);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::FocusChanged(_) | Update::ChildFocusChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let side = cell_side(&self.theme);
        let cols = self.cols.max(1);
        let n = self.cells.len();
        let rows = n.div_ceil(cols);
        match axis {
            Axis::Horizontal => Length::px(index_f64(cols) * side),
            Axis::Vertical => Length::px(index_f64(rows) * side),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let cell_px = cell_side(&self.theme);
        let cols = self.cols.max(1);
        // Columns stretch to fill whatever width the parent granted (it may
        // be wider than `cols * cell_px` — e.g. Year mode, where the header's
        // "2020–2039" label needs more room than 4 grid columns do), so the
        // grid never leaves dead space to the right of a narrower parent.
        let col_w = (size.width / index_f64(cols)).max(cell_px);
        let cell_size = Size::new(col_w, cell_px);

        let n = self.cells.len();

        self.cell_rects.clear();
        self.cell_rects.reserve(n);

        for (i, pod) in self.cells.iter_mut().enumerate() {
            let col = i % cols;
            let row = i / cols;

            let col_f = index_f64(col);
            let row_f = index_f64(row);

            let origin = Point::new(col_f * col_w, row_f * cell_px);
            let rect = Rect::from_origin_size(origin, cell_size);
            self.cell_rects.push(rect);

            // Center the label's own natural size within the cell, rather
            // than stretching it to fill the cell — a label placed at cell
            // origin and forced to the full cell size sits top/left-anchored
            // (Label doesn't center within an oversized box), which reads as
            // a large gap below/right of each row of text.
            let natural = ctx.compute_size(pod, SizeDef::MIN, LayoutSize::from(cell_size));
            ctx.run_layout(pod, natural);
            let label_origin = origin
                + Vec2::new(
                    ((cell_size.width - natural.width) / 2.0).max(0.0),
                    ((cell_size.height - natural.height) / 2.0).max(0.0),
                );
            ctx.place_child(pod, label_origin);
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;
        let radius = f64::from(self.theme.radius.small);

        for (i, (&rect, datum)) in self.cell_rects.iter().zip(self.data.iter()).enumerate() {
            // ── Background fill ──────────────────────────────────────────────
            if datum.selected {
                let rrect = RoundedRect::from_rect(rect, radius);
                painter.fill(rrect, p.accent).draw();
            } else if self.hover_index == Some(i) && !datum.disabled {
                let rrect = RoundedRect::from_rect(rect, radius);
                painter.fill(rrect, p.accent_soft).draw();
            }

            // ── Today dot ────────────────────────────────────────────────────
            if datum.today && !datum.selected {
                let cx = rect.center().x;
                let cy = rect.y1 - TODAY_DOT_MARGIN_B;
                painter
                    .fill(Circle::new((cx, cy), TODAY_DOT_RADIUS), p.accent)
                    .draw();
            }

            // ── Focus ring ───────────────────────────────────────────────────
            if self.focused_index == Some(i) {
                let inset = FOCUS_RING_INSET;
                let inset_rect = rect.inset(Insets::uniform(-inset));
                let rrect = RoundedRect::from_rect(inset_rect, (radius - inset).max(0.0));
                paint_focus_ring(painter, rrect, &self.theme);
            }
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // ── Hover tracking ───────────────────────────────────────────────────
        if let PointerEvent::Move(PointerUpdate { current, .. }) = event {
            let local = to_local(ctx, current.logical_point());
            let new_hover = hit_item(&self.cell_rects, local);
            if new_hover != self.hover_index {
                self.hover_index = new_hover;
                ctx.request_paint_only();
            }
            return;
        }
        if let PointerEvent::Leave(_) = event {
            if self.hover_index.is_some() {
                self.hover_index = None;
                ctx.request_paint_only();
            }
            return;
        }

        // ── Click recognizer ─────────────────────────────────────────────────
        let Some(phase) = primary_click_when(ctx, event, |ctx, state| {
            // Only capture if the press landed on a cell.
            !self.cell_rects.is_empty()
                && hit_item(&self.cell_rects, to_local(ctx, state.logical_point())).is_some()
        }) else {
            return;
        };

        match phase {
            ClickPhase::Down(state) => {
                let local = to_local(ctx, state.logical_point());
                if let Some(i) = hit_item(&self.cell_rects, local) {
                    self.hover_index = Some(i);
                }
                ctx.request_focus();
                ctx.request_paint_only();
            }
            ClickPhase::Up { state, completed } => {
                if completed {
                    let local = to_local(ctx, state.logical_point());
                    if let Some(i) = hit_item(&self.cell_rects, local)
                        && !self.data.get(i).is_some_and(|d| d.disabled)
                    {
                        ctx.submit_action::<Self::Action>(CalendarGridAction::CellActivated(i));
                    }
                }
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Grid
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
        // Each child `CalendarCell` declares itself as a `Role::GridCell`
        // with its own selected/disabled state. Minimal parent node — row/
        // column counts could be set here.
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.cells.iter().map(WidgetPod::id).collect();
        ChildrenIds::from_slice(&ids)
    }
}
