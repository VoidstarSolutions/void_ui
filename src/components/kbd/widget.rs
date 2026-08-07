//! Masonry widget for `kbd` — a raised keycap painted behind a mono label.
//!
//! Presentation only: no pointer/keyboard interaction, no emitted actions.
//! The keycap is two stacked rounded rects painted with masonry's own
//! [`paint_background`] helper (as `meter` does): a darker outer rect that
//! shows through as a 1px border ring plus a `LIP_PX` bottom "lip", and a
//! lighter body rect inset on top of it. Reads [`Theme`] as a value (no
//! `Theme` is reachable through the property stack — see `button/widget.rs`).
//!
//! The label text is laid out with parley directly (rather than wrapping a
//! masonry [`Label`]) so a *styled run* can put the keyboard-symbol glyphs
//! (⌘/⇧/⌫/→ …) on a symbol-capable font while the ASCII key letters stay on
//! the mono stack. masonry's `Label` only carries default styles, so it can't
//! express a per-range font, and parley's list-fallback doesn't reliably walk
//! to the symbol face at the tail of the mono stack — so those glyphs would
//! render as tofu. See [`symbol_runs`].

use std::borrow::Cow;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, BrushIndex, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetMut,
    paint_background, render_text,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::parley::style::GenericFamily;
use masonry::parley::{Alignment, AlignmentOptions, FontFamily, FontFamilyName, Layout};
use masonry::peniko::{Brush, Color};
use masonry::properties::{Background, BorderWidth, CornerRadius};

use crate::Theme;

/// Bottom "lip" thickness in px — the exposed dark edge below the body that
/// gives the keycap its raised, physical-key look.
const LIP_PX: f64 = 2.0;

/// Byte ranges of maximal runs of non-ASCII characters in `text`.
///
/// Every keyboard-symbol glyph (⌘ ⇧ ⌫ → …) is non-ASCII while the plain key
/// labels (`S`, `Enter`, `F5`) and the platform modifier *words* (`Shift`,
/// `Ctrl`) are ASCII — so these runs are exactly the spans that must be
/// styled onto the symbol font. The thin-space separator (U+2009) is
/// non-ASCII too, so an adjacent `⇧⌘` group is one run; styling that
/// whitespace with the symbol font is harmless (it has no ink).
fn symbol_runs(text: &str) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        if ch.is_ascii() {
            if let Some(s) = start.take() {
                runs.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start.take() {
        runs.push((s, text.len()));
    }
    runs
}

/// Wrap a parsed family list in a parley [`FontFamily`], falling back to the
/// bare `Monospace` generic when the list is empty.
fn family_list(families: &[FontFamilyName<'static>]) -> FontFamily<'static> {
    if families.is_empty() {
        FontFamily::Single(FontFamilyName::Generic(GenericFamily::Monospace))
    } else {
        FontFamily::List(Cow::Owned(families.to_vec()))
    }
}

/// A raised keycap chip rendering a single line of mono/symbol text.
pub struct KbdWidget {
    text: ArcStr,
    spoken_name: ArcStr,
    theme: Theme,
    /// Mono family stack applied to the whole line by default.
    mono_families: Vec<FontFamilyName<'static>>,
    /// Symbol-first family stack applied as a styled run over each
    /// [`symbol_runs`] span so keyboard glyphs resolve instead of tofu-ing.
    symbol_families: Vec<FontFamilyName<'static>>,
    font_size: f32,
    text_color: Color,
    layout: Layout<BrushIndex>,
    /// Top-left origin the text is painted at, set in `layout()`.
    text_origin: Point,
    /// Cleared the next time `ensure_layout` rebuilds the parley layout.
    layout_dirty: bool,
}

impl KbdWidget {
    pub(super) fn new(
        text: ArcStr,
        theme: &Theme,
        spoken_name: ArcStr,
        mono_families: Vec<FontFamilyName<'static>>,
        symbol_families: Vec<FontFamilyName<'static>>,
    ) -> Self {
        Self {
            text,
            spoken_name,
            theme: *theme,
            mono_families,
            symbol_families,
            font_size: theme.typography.size_body,
            text_color: theme.palette.text,
            layout: Layout::default(),
            text_origin: Point::ORIGIN,
            layout_dirty: true,
        }
    }

    pub(super) fn set_text(this: &mut WidgetMut<'_, Self>, text: ArcStr) {
        if this.widget.text != text {
            this.widget.text = text;
            this.widget.layout_dirty = true;
            this.ctx.request_layout();
        }
    }

    pub(super) fn set_spoken_name(this: &mut WidgetMut<'_, Self>, name: ArcStr) {
        if this.widget.spoken_name != name {
            this.widget.spoken_name = name;
            this.ctx.request_render();
        }
    }

    pub(super) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.widget.font_size = theme.typography.size_body;
            this.widget.text_color = theme.palette.text;
            this.widget.layout_dirty = true;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the mono + symbol family stacks (theme typography changed).
    pub(super) fn set_fonts(
        this: &mut WidgetMut<'_, Self>,
        mono_families: Vec<FontFamilyName<'static>>,
        symbol_families: Vec<FontFamilyName<'static>>,
    ) {
        if this.widget.mono_families != mono_families
            || this.widget.symbol_families != symbol_families
        {
            this.widget.mono_families = mono_families;
            this.widget.symbol_families = symbol_families;
            this.widget.layout_dirty = true;
            this.ctx.request_layout();
        }
    }

    /// The origin the text is painted at — exposed for the relayout test.
    #[cfg(test)]
    pub(crate) fn text_origin(&self) -> Point {
        self.text_origin
    }

    /// The raised-key surface (body) color.
    fn body_color(&self) -> Color {
        self.theme.palette.surface_hi
    }

    /// The border ring + bottom-lip color.
    fn edge_color(&self) -> Color {
        self.theme.palette.border_strong
    }

    fn corner_px(&self) -> f64 {
        f64::from(self.theme.radius.small)
    }

    /// Rebuilds the parley layout when dirty. The chip is always a single
    /// unwrapped line, so there is no max-advance to track.
    fn ensure_layout(
        &mut self,
        font_ctx: &mut masonry::parley::FontContext,
        layout_ctx: &mut masonry::parley::LayoutContext<BrushIndex>,
    ) {
        if !self.layout_dirty {
            return;
        }
        let mut builder = layout_ctx.ranged_builder(font_ctx, &self.text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(self.font_size));
        builder.push_default(StyleProperty::FontFamily(family_list(&self.mono_families)));
        builder.push_default(StyleProperty::Brush(BrushIndex(0)));
        // Styled run: put the keyboard-symbol glyph spans on the symbol stack.
        for (start, end) in symbol_runs(&self.text) {
            builder.push(
                StyleProperty::FontFamily(family_list(&self.symbol_families)),
                start..end,
            );
        }
        builder.build_into(&mut self.layout, &self.text);
        self.layout.break_all_lines(None);
        self.layout
            .align(None, Alignment::Start, AlignmentOptions::default());
        self.layout_dirty = false;
    }
}

impl Widget for KbdWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _type_id: std::any::TypeId) {}

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
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let (font_ctx, layout_ctx) = ctx.text_contexts();
        self.ensure_layout(font_ctx, layout_ctx);
        match axis {
            Axis::Horizontal => Length::px(f64::from(self.layout.width()) + 2.0 * pad_h),
            Axis::Vertical => Length::px(f64::from(self.layout.height()) + 2.0 * pad_v + LIP_PX),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let (font_ctx, layout_ctx) = ctx.text_contexts();
        self.ensure_layout(font_ctx, layout_ctx);

        // Center the text on the key face (the padded region above the lip),
        // both axes — a stretched box would leave the mono line top-aligned
        // (visibly high) whenever the chip is taller than the text.
        let text_w = f64::from(self.layout.width());
        let text_h = f64::from(self.layout.height());
        let face_w = (size.width - 2.0 * pad_h).max(0.0);
        let face_h = (size.height - 2.0 * pad_v - LIP_PX).max(0.0);
        let x = pad_h + ((face_w - text_w) * 0.5).max(0.0);
        let y = pad_v + ((face_h - text_h) * 0.5).max(0.0);
        self.text_origin = Point::new(x, y);

        if let Some(line) = self.layout.get(0) {
            let baseline = y + f64::from(line.metrics().baseline);
            ctx.set_baselines(baseline, baseline);
        } else {
            ctx.clear_baselines();
        }
    }

    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let bbox = ctx.border_box();
        let radius = CornerRadius::all(Length::px(self.corner_px()));
        // 1. Outer darker rect — becomes the 1px border ring + the LIP_PX
        //    bottom lip once the body is painted on top.
        paint_background(
            painter,
            bbox,
            &Background::Color(self.edge_color()),
            &BorderWidth::default(),
            &radius,
        );
        // 2. Body — inset 1px top/left/right and (1 + LIP_PX) bottom, so the
        //    dark outer shows as a hairline ring plus a thicker bottom edge.
        // Clamped so a host-forced undersized box (smaller than ~4px) can't
        // invert the rect (max coords are never less than the min coords).
        let x1 = (bbox.x1 - 1.0).max(bbox.x0 + 1.0);
        let y1 = (bbox.y1 - 1.0 - LIP_PX).max(bbox.y0 + 1.0);
        let body = Rect::new(bbox.x0 + 1.0, bbox.y0 + 1.0, x1, y1);
        let inner_radius = CornerRadius::all(Length::px((self.corner_px() - 1.0).max(0.0)));
        paint_background(
            painter,
            body,
            &Background::Color(self.body_color()),
            &BorderWidth::default(),
            &inner_radius,
        );
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let brushes = [Brush::Solid(self.text_color)];
        render_text(
            painter,
            Affine::translate((self.text_origin.x, self.text_origin.y)),
            &self.layout,
            &brushes,
            true,
        );
    }

    fn accessibility_role(&self) -> Role {
        Role::Label
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.spoken_name.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use masonry::accesskit::Role;
    use masonry::core::{ArcStr, NewWidget};
    use masonry::parley::FontFamilyName;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    use super::{KbdWidget, symbol_runs};
    use crate::Theme;

    /// Parse the mono + symbol stacks the view would build, for realistic
    /// glyph coverage in the harness.
    fn stacks() -> (Vec<FontFamilyName<'static>>, Vec<FontFamilyName<'static>>) {
        let theme = Theme::default();
        let mono: Vec<_> = theme
            .typography
            .mono
            .families
            .iter()
            .filter_map(|f| FontFamilyName::parse(f))
            .collect();
        let mut symbol: Vec<_> = ["Apple Symbols", "Segoe UI Symbol"]
            .iter()
            .filter_map(|f| FontFamilyName::parse(f))
            .collect();
        symbol.extend(mono.clone());
        (mono, symbol)
    }

    fn harness(text: &str, spoken: &str) -> TestHarness<KbdWidget> {
        let theme = Theme::default();
        let (mono, symbol) = stacks();
        let widget = KbdWidget::new(
            ArcStr::from(text),
            &theme,
            ArcStr::from(spoken),
            mono,
            symbol,
        );
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (200, 40))
    }

    #[test]
    fn symbol_runs_split_ascii_from_glyphs() {
        // Bare key: no runs.
        assert_eq!(symbol_runs("Enter"), vec![]);
        // Bare glyph key: whole string is one run.
        assert_eq!(symbol_runs("⇧"), vec![(0, "⇧".len())]);
        // Modifiers + thin space + ASCII key: the glyph+separator prefix is one
        // run, the trailing ASCII key is excluded.
        let s = "⇧\u{2009}⌘\u{2009}K";
        let key_start = s.len() - "K".len();
        assert_eq!(symbol_runs(s), vec![(0, key_start)]);
    }

    #[test]
    fn mounts_and_paints_without_panicking() {
        for (text, spoken) in [("K", "K"), ("⇧\u{2009}⌘\u{2009}K", "Shift Command K")] {
            let mut h = harness(text, spoken);
            h.redraw();
        }
    }

    #[test]
    fn reports_label_role_and_spoken_name() {
        let mut h = harness("⌘\u{2009}K", "Command K");
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.label().as_deref(), Some("Command K"));
    }

    #[test]
    fn set_spoken_name_updates_accessibility() {
        let mut h = harness("⌘\u{2009}K", "Command K");
        h.redraw();
        h.edit_root_widget(|mut wm| {
            KbdWidget::set_spoken_name(&mut wm, ArcStr::from("Command J"));
        });
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.label().as_deref(), Some("Command J"));
    }

    /// `set_theme` must request layout (not just repaint) when density
    /// changes, since `button_pad_v`/`button_pad_h` feed `measure`/`layout`.
    /// The root widget itself always fills the harness window (tight
    /// constraints), so the observable is the text paint origin, which
    /// `layout` derives from `pad_h`/`pad_v` — a post-mount theme swap to a
    /// wider density must shift it.
    #[test]
    fn set_theme_with_new_density_relayouts() {
        let mut h = harness("⌘\u{2009}K", "Command K");
        h.redraw();
        let origin = |h: &TestHarness<KbdWidget>| {
            h.root_widget()
                .downcast::<KbdWidget>()
                .expect("root is KbdWidget")
                .text_origin()
        };
        let before = origin(&h);

        let mut grown = Theme::default();
        grown.density.button_pad_h += 20.0;
        grown.density.button_pad_v += 20.0;
        h.edit_root_widget(|mut wm| {
            KbdWidget::set_theme(&mut wm, &grown);
        });
        h.redraw();
        let after = origin(&h);

        assert_ne!(
            before, after,
            "set_theme should trigger a relayout when density padding changes"
        );
    }
}
