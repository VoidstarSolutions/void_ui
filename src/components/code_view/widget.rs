//! Masonry widget for the read-only highlighted text field.
//!
//! Owns a `String` and a parley [`Layout<BrushIndex>`]. Paints text using a
//! brush palette indexed by `BrushIndex(n)` — index 0 is the fallback
//! (`CodePalette::plain`), indices 1.. are the per-token-kind colors.
//!
//! IME is deferred to a later task; this widget supports focus-on-click,
//! click/drag selection (single/double/triple + shift-extend), and
//! Ctrl/Cmd+C copy.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState};
use masonry::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx,
    NoAction, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, StyleProperty, TextEvent, Update,
    UpdateCtx, Widget, WidgetId, WidgetMut, render_text,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{AsUnit, LenReq, Length};
use masonry::parley::style::GenericFamily;
use masonry::parley::{
    Alignment, AlignmentOptions, Cursor, FontFamily, FontFamilyName, Layout, Selection,
};
use masonry::peniko::{Brush, Color};
use tracing::{Span, trace_span};

use super::highlighter::{TokenKind, TokenSpan};

/// Number of brushes in the palette passed to `render_text`. Index 0 is the
/// fallback (`CodePalette::plain`); indices 1..=9 are the per-token-kind colors
/// produced by [`brush_index_for_kind`].
pub(crate) const BRUSH_PALETTE_LEN: usize = 10;

/// Returns the brush palette index for a given [`TokenKind`].
///
/// Index 0 is reserved for the fallback color and is never returned here.
/// This mapping is the contract between [`CodeViewWidget`] and the view that
/// builds its brush palette; keep them in sync.
fn brush_index_for_kind(kind: TokenKind) -> u32 {
    match kind {
        TokenKind::Keyword => 1,
        TokenKind::Type => 2,
        TokenKind::Function => 3,
        TokenKind::Identifier => 4,
        TokenKind::String => 5,
        TokenKind::Number => 6,
        TokenKind::Comment => 7,
        TokenKind::Punctuation => 8,
        TokenKind::Operator => 9,
    }
}

/// Read-only highlighted code view.
///
/// Drives parley's [`Layout<BrushIndex>`] directly so it can apply per-range
/// brush colors. Supports focus-on-click, click/drag selection (single /
/// double=word / triple=line / shift-extend), and Ctrl/Cmd+C copy. IME is
/// intentionally omitted — this widget never edits text.
pub struct CodeViewWidget {
    text: String,
    spans: Vec<TokenSpan>,
    /// Exactly [`BRUSH_PALETTE_LEN`] brushes. `brushes[0]` is the
    /// plain/fallback color; `brushes[1..]` follow [`brush_index_for_kind`].
    brushes: Vec<Brush>,
    background: Color,
    border_color: Color,
    border_width: f32,
    corner_radius: f32,
    padding: f32,
    /// Extra space reserved on the right side beyond `padding`, used to avoid
    /// overlap with overlaid affordances (e.g. the clipboard button).
    right_inset: f32,
    font_size: f32,
    /// Fill color for selection rectangles painted under the text.
    selection_color: Color,
    /// Working layout — mutated freely by `measure()` for whatever query is
    /// being made. Don't read this in `paint` or pointer code; its
    /// `break_all_lines` state may not match what's on screen.
    layout: Layout<BrushIndex>,
    /// Snapshot of `layout` taken at the end of `layout()` — this is the
    /// state that matches the widget's currently-assigned rect. `paint` and
    /// pointer/selection code read from this so they aren't affected by
    /// post-layout measure passes (e.g. a `MinContent` query that re-breaks
    /// `layout` to one-word-per-line).
    paint_layout: Layout<BrushIndex>,
    /// Current selection. Collapsed (`is_collapsed()`) by default; updated by
    /// pointer events.
    selection: Selection,
    /// `Some(w)` means the cached layout was broken with max-advance `w`;
    /// `None` means it was unbroken or has never been built.
    last_max_advance: Option<f32>,
    /// Set on construction and whenever inputs that affect glyph
    /// composition change; cleared the next time we run `rebuild_layout`.
    layout_dirty: bool,
}

impl CodeViewWidget {
    /// Builds a new widget. `brushes` must have exactly [`BRUSH_PALETTE_LEN`]
    /// entries — see [`brush_index_for_kind`] for the index assignment.
    ///
    /// # Panics
    ///
    /// Panics if `brushes.len()` is not [`BRUSH_PALETTE_LEN`].
    #[expect(
        clippy::too_many_arguments,
        reason = "render-only widget needs all chrome + text inputs up front; \
                  splitting would just push the noise to the call site"
    )]
    #[must_use]
    pub fn new(
        text: String,
        spans: Vec<TokenSpan>,
        brushes: Vec<Brush>,
        background: Color,
        border_color: Color,
        border_width: f32,
        corner_radius: f32,
        padding: f32,
        right_inset: f32,
        font_size: f32,
        selection_color: Color,
    ) -> Self {
        assert_eq!(
            brushes.len(),
            BRUSH_PALETTE_LEN,
            "brush palette must have exactly {BRUSH_PALETTE_LEN} entries"
        );
        Self {
            text,
            spans,
            brushes,
            background,
            border_color,
            border_width,
            corner_radius,
            padding,
            right_inset,
            font_size,
            selection_color,
            layout: Layout::default(),
            paint_layout: Layout::default(),
            selection: Selection::default(),
            last_max_advance: None,
            layout_dirty: true,
        }
    }
}

// --- MARK: WIDGETMUT
impl CodeViewWidget {
    /// Replaces the source text. Marks the layout dirty and requests layout if
    /// the value changed.
    ///
    /// Collapses the selection: its byte offsets are meaningless against the
    /// new buffer, and a stale range could paint or copy the wrong bytes.
    pub fn set_text(this: &mut WidgetMut<'_, Self>, text: String) {
        if this.widget.text != text {
            this.widget.text = text;
            this.widget.selection = Selection::default();
            this.widget.layout_dirty = true;
            this.ctx.request_layout();
        }
    }

    /// Replaces the highlighted spans. Marks the layout dirty and requests
    /// layout if the value changed.
    pub fn set_spans(this: &mut WidgetMut<'_, Self>, spans: Vec<TokenSpan>) {
        if this.widget.spans != spans {
            this.widget.spans = spans;
            this.widget.layout_dirty = true;
            this.ctx.request_layout();
        }
    }

    /// Replaces the brush palette. Indexed by [`brush_index_for_kind`].
    /// Repaint only — the glyph layout is unchanged.
    ///
    /// # Panics
    ///
    /// Panics if `brushes.len()` is not [`BRUSH_PALETTE_LEN`].
    pub fn set_brushes(this: &mut WidgetMut<'_, Self>, brushes: Vec<Brush>) {
        assert_eq!(
            brushes.len(),
            BRUSH_PALETTE_LEN,
            "brush palette must have exactly {BRUSH_PALETTE_LEN} entries"
        );
        this.widget.brushes = brushes;
        // Paint-only; skip deep-eq on Vec<Brush> since the view controls churn.
        this.ctx.request_paint_only();
    }

    /// Replaces the chrome (background, border, corner, padding). Only
    /// `padding` requests a relayout (since `measure`/`layout` consume it);
    /// `border_width` is paint-only along with the other chrome inputs.
    pub fn set_chrome(
        this: &mut WidgetMut<'_, Self>,
        background: Color,
        border_color: Color,
        border_width: f32,
        corner_radius: f32,
        padding: f32,
        selection_color: Color,
    ) {
        let w = &mut *this.widget;
        let layout_changed = (w.padding - padding).abs() > f32::EPSILON;
        w.background = background;
        w.border_color = border_color;
        w.border_width = border_width;
        w.corner_radius = corner_radius;
        w.padding = padding;
        w.selection_color = selection_color;
        if layout_changed {
            this.ctx.request_layout();
        } else {
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the font size. Marks the layout dirty and requests layout if
    /// the value changed.
    pub fn set_font_size(this: &mut WidgetMut<'_, Self>, font_size: f32) {
        if (this.widget.font_size - font_size).abs() > f32::EPSILON {
            this.widget.font_size = font_size;
            this.widget.layout_dirty = true;
            this.ctx.request_layout();
        }
    }

    /// Sets extra space reserved on the right side of the text area beyond the
    /// normal padding. Use this to avoid text overlapping an overlaid affordance
    /// such as the clipboard button. Requests layout if changed.
    pub fn set_right_inset(this: &mut WidgetMut<'_, Self>, right_inset: f32) {
        if (this.widget.right_inset - right_inset).abs() > f32::EPSILON {
            this.widget.right_inset = right_inset;
            this.ctx.request_layout();
        }
    }
}

// --- MARK: LAYOUT BUILDING
impl CodeViewWidget {
    /// Rebuilds the parley `Layout` from the current text + spans + font size
    /// using the supplied parley contexts. Always re-runs `break_all_lines`
    /// and `align` for the requested `max_advance`.
    fn rebuild_layout(
        &mut self,
        font_ctx: &mut masonry::parley::FontContext,
        layout_ctx: &mut masonry::parley::LayoutContext<BrushIndex>,
        max_advance: Option<f32>,
    ) {
        let mut builder = layout_ctx.ranged_builder(font_ctx, &self.text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(self.font_size));
        builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Generic(GenericFamily::Monospace),
        )));
        builder.push_default(StyleProperty::Brush(BrushIndex(0)));
        // Spans come from a pluggable `Highlighter`, so a third-party impl may
        // hand us empty or out-of-bounds ranges. Skip empty/inverted ranges and
        // clamp the end to the text length; overlapping spans are fine (a later
        // push overrides an earlier one).
        let text_len = self.text.len();
        for span in &self.spans {
            let start = span.range.start;
            let end = span.range.end.min(text_len);
            if start >= end {
                continue;
            }
            builder.push(
                StyleProperty::Brush(BrushIndex(brush_index_for_kind(span.kind) as usize)),
                start..end,
            );
        }
        builder.build_into(&mut self.layout, &self.text);
        self.layout.break_all_lines(max_advance);
        self.layout
            .align(max_advance, Alignment::Start, AlignmentOptions::default());
        self.last_max_advance = max_advance;
        self.layout_dirty = false;
    }

    /// Refreshes the layout for the given `max_advance`, reusing the cached
    /// layout whenever inputs are unchanged.
    fn ensure_layout(
        &mut self,
        font_ctx: &mut masonry::parley::FontContext,
        layout_ctx: &mut masonry::parley::LayoutContext<BrushIndex>,
        max_advance: Option<f32>,
    ) {
        if self.layout_dirty {
            self.rebuild_layout(font_ctx, layout_ctx, max_advance);
            return;
        }
        if self.last_max_advance != max_advance {
            self.layout.break_all_lines(max_advance);
            self.layout
                .align(max_advance, Alignment::Start, AlignmentOptions::default());
            self.last_max_advance = max_advance;
        }
    }
}

// --- MARK: POINTER HELPERS
impl CodeViewWidget {
    /// Translates a window-local `Point` into layout-local `(x, y)` coordinates
    /// by subtracting the chrome padding and converting f64 → f32.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "kurbo Point is f64; parley layout is f32 (UI scale, precision is fine)."
    )]
    fn layout_local(&self, pos: Point) -> (f32, f32) {
        let pad = f64::from(self.padding);
        ((pos.x - pad) as f32, (pos.y - pad) as f32)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for CodeViewWidget {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        // Keyboard copy (Ctrl/Cmd+C) needs focus; clicking the text requests
        // it in `on_pointer_event`.
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FontsChanged) {
            self.layout_dirty = true;
            ctx.request_layout();
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
        let inline = Axis::Horizontal;
        let pad_each = f64::from(self.padding);
        let pad_main = 2.0 * pad_each;
        // Total horizontal inset: normal padding on both sides plus any extra
        // space reserved on the right for overlaid affordances (e.g. the
        // clipboard button).
        let h_inset = pad_main + f64::from(self.right_inset);

        // Fill the offered width: a code block is a block-level panel that
        // owns its chrome, so the painted background must match the widget
        // bounds exactly (overlaid affordances like the copy button align to
        // those bounds). Content-based sizing still answers Min/MaxContent.
        if axis == inline
            && let LenReq::FitContent(space) = len_req
        {
            return space;
        }

        // Determine the max-advance for the text layout, mirroring the
        // word-wrap-style branching in masonry's `Label::measure`.
        let max_advance = if axis == inline {
            match len_req {
                LenReq::MinContent => Some(Length::ZERO),
                LenReq::MaxContent => None,
                LenReq::FitContent(space) => Some(Length::px((space.get() - h_inset).max(0.0))),
            }
        } else {
            // Block axis depends on the cross (inline) length.
            cross_length.map(|c| Length::px((c.get() - h_inset).max(0.0)))
        }
        .map(|v| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "parley layout width is f32; truncation of a UI-scale length is acceptable"
            )]
            let px = v.get() as f32;
            px
        });

        let (font_ctx, layout_ctx) = ctx.text_contexts();
        self.ensure_layout(font_ctx, layout_ctx, max_advance);

        let raw = if axis == inline {
            f64::from(self.layout.width())
        } else {
            f64::from(self.layout.height())
        };
        (raw + pad_main).px()
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_each = f64::from(self.padding);
        let inner_max_w = {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "parley layout width is f32; truncation of a UI-scale length is acceptable"
            )]
            let w = (size.width - 2.0 * pad_each - f64::from(self.right_inset)).max(0.0) as f32;
            w
        };

        // `ensure_layout` re-breaks the cached layout when a prior measure()
        // pass left it at a different max_advance (tracked via
        // `last_max_advance`) and fully rebuilds only when glyph-affecting
        // inputs changed (`layout_dirty`) — so paint() always sees the state
        // matching the assigned `size` without paying for a rebuild on every
        // relayout.
        let (font_ctx, layout_ctx) = ctx.text_contexts();
        self.ensure_layout(font_ctx, layout_ctx, Some(inner_max_w));

        // Snapshot the committed layout for paint() and pointer events to use.
        // Later measure() passes may re-break `self.layout` (e.g. a MinContent
        // query with max_advance=0 wraps to many lines), but they must not
        // affect what gets painted.
        self.paint_layout = self.layout.clone();

        let line_count = self.paint_layout.len();
        if line_count > 0 {
            let first = self.paint_layout.get(0).unwrap();
            let last = self.paint_layout.get(line_count - 1).unwrap();
            let first_baseline = pad_each + f64::from(first.metrics().baseline);
            let last_baseline = pad_each + f64::from(last.metrics().baseline);
            ctx.set_baselines(first_baseline, last_baseline);
        } else {
            ctx.clear_baselines();
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box().size();
        let rect =
            RoundedRect::from_origin_size(Point::ORIGIN, size, f64::from(self.corner_radius));

        if self.background.components[3] > 0.0 {
            painter.fill(rect, self.background).draw();
        }
        if self.border_width > 0.0 && self.border_color.components[3] > 0.0 {
            painter
                .stroke(
                    rect,
                    &Stroke::new(f64::from(self.border_width)),
                    self.border_color,
                )
                .draw();
        }

        // Paint selection rectangles under the text (no-op when collapsed).
        if self.selection_color.components[3] > 0.0 {
            let pad = f64::from(self.padding);
            for (bbox, _line_index) in self.selection.geometry(&self.paint_layout) {
                let sel_rect =
                    Rect::new(bbox.x0 + pad, bbox.y0 + pad, bbox.x1 + pad, bbox.y1 + pad);
                painter.fill(sel_rect, self.selection_color).draw();
            }
        }

        render_text(
            painter,
            Affine::translate((f64::from(self.padding), f64::from(self.padding))),
            &self.paint_layout,
            &self.brushes,
            true,
        );
    }

    fn accessibility_role(&self) -> Role {
        Role::Document
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_read_only();
        node.set_value(self.text.clone());
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let (x, y) = self.layout_local(ctx.local_position(state.position));
                match state.count {
                    2 => {
                        self.selection = Selection::word_from_point(&self.paint_layout, x, y);
                    }
                    3 => {
                        self.selection = Selection::hard_line_from_point(&self.paint_layout, x, y);
                    }
                    _ => {
                        if state.modifiers.shift() {
                            self.selection =
                                self.selection
                                    .shift_click_extension(&self.paint_layout, x, y);
                        } else {
                            // Two-step (Cursor::from_point + Selection::from) instead of
                            // Selection::from_point so cursor affinity stays explicit.
                            let cursor = Cursor::from_point(&self.paint_layout, x, y);
                            self.selection = Selection::from(cursor);
                        }
                    }
                }
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                let (x, y) = self.layout_local(ctx.local_position(current.position));
                self.selection = self.selection.extend_to_point(&self.paint_layout, x, y);
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key_event) = event else {
            return;
        };
        if key_event.state != KeyState::Down {
            return;
        }
        let action_mod = if cfg!(target_os = "macos") {
            key_event.modifiers.meta()
        } else {
            key_event.modifiers.ctrl()
        };
        if action_mod
            && matches!(&key_event.key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("c"))
        {
            let range = self.selection.text_range();
            // `get` instead of indexing: parley ranges should always be
            // char-boundary aligned, but a panic inside an event handler is
            // a bad failure mode for a defensive check this cheap.
            if !range.is_empty()
                && let Some(text) = self.text.get(range)
            {
                ctx.set_clipboard(text.to_string());
            }
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn get_cursor(&self, _ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
        CursorIcon::Text
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("CodeViewWidget", id = id.trace())
    }
}
