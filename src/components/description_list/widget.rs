//! Masonry widget for the description list component.
//!
//! [`DescriptionListWidget`] is a multi-child container holding one label pod and
//! one value pod per item. In horizontal mode it measures every label, sets the
//! shared label-column width to the widest, and baseline-aligns each value against
//! its label when both expose a real text baseline — falling back to top-align
//! when either doesn't (e.g. a non-text value like a status dot). In stacked mode
//! each value is laid out directly below its label.
//!
//! Both layout modes are real: horizontal (`layout_horizontal`/`measure`'s
//! Horizontal arm) as of Task 3, stacked (`layout_stacked`/`measure_stacked`) as
//! of Task 4.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::widgets::Passthrough;

use super::view::DescriptionListOrientation;
use crate::Theme;

pub struct DescriptionListWidget {
    labels: Vec<WidgetPod<Passthrough>>,
    values: Vec<WidgetPod<Passthrough>>,
    orientation: DescriptionListOrientation,
    theme: Theme,
}

// --- MARK: BUILDERS

impl DescriptionListWidget {
    /// # Panics
    ///
    /// Panics if `labels` and `values` don't have the same length.
    pub(super) fn new(
        labels: Vec<NewWidget<Passthrough>>,
        values: Vec<NewWidget<Passthrough>>,
        orientation: DescriptionListOrientation,
        theme: &Theme,
    ) -> Self {
        assert_eq!(
            labels.len(),
            values.len(),
            "DescriptionListWidget: labels and values must be the same length"
        );
        Self {
            labels: labels.into_iter().map(NewWidget::to_pod).collect(),
            values: values.into_iter().map(NewWidget::to_pod).collect(),
            orientation,
            theme: *theme,
        }
    }
}

// --- MARK: LAYOUT HELPERS

impl DescriptionListWidget {
    /// Horizontal gap between the label column and the value column.
    fn column_gap(&self) -> f64 {
        f64::from(self.theme.density.gap_lg)
    }
    /// Vertical gap between successive rows / items.
    fn row_gap(&self) -> f64 {
        f64::from(self.theme.density.gap)
    }
    /// Vertical gap between a label and its value in stacked mode.
    fn pair_gap(&self) -> f64 {
        f64::from(self.theme.density.pad)
    }

    /// Slack for detecting masonry's baseline-substitution sentinel: a widget
    /// that never calls `set_baselines` gets its layout-first-baseline reported
    /// as exactly its own border-box height (see `layout_horizontal`'s
    /// `*_has_base` comment). A tiny epsilon absorbs float noise without risking
    /// a false "real baseline" match for a genuine text baseline that happens to
    /// sit very close to the child's bottom edge.
    const BASELINE_SENTINEL_EPS: f64 = 0.5;

    fn layout_horizontal(&mut self, ctx: &mut LayoutCtx<'_>, size: Size) {
        let ctx_size = LayoutSize::from(size);

        // Pass 1: widest label sets the shared column width.
        let mut col_w = 0.0_f64;
        let mut label_pref: Vec<Size> = Vec::with_capacity(self.labels.len());
        for label in &mut self.labels {
            let s = ctx.compute_size(label, SizeDef::MIN, ctx_size);
            col_w = col_w.max(s.width);
            label_pref.push(s);
        }

        let column_gap = self.column_gap();
        let row_gap = self.row_gap();
        let value_x = col_w + column_gap;
        let value_avail = (size.width - value_x).max(0.0);

        // Pass 2: lay out each row, baseline-aligning value to label.
        let mut y = 0.0_f64;
        let rows = self
            .labels
            .iter_mut()
            .zip(self.values.iter_mut())
            .zip(label_pref.iter());
        for ((label, value), lp) in rows {
            let lh = lp.height;
            ctx.run_layout(label, Size::new(col_w, lh));
            // `child_layout_baselines` (not `child_aligned_baselines`) is the one that
            // only requires `run_layout` — it's meant for deciding placement, whereas
            // the "aligned" variant asserts the child has already been placed.
            let (label_base, _) = ctx.child_layout_baselines(label);

            let vs = ctx.compute_size(
                value,
                SizeDef::MIN,
                LayoutSize::from(Size::new(value_avail, size.height)),
            );
            ctx.run_layout(value, vs);
            let (value_base, _) = ctx.child_layout_baselines(value);

            // Baseline align only when BOTH children expose a real text baseline;
            // otherwise top-align. `child_layout_baselines` never returns NaN:
            // masonry substitutes the child's own border-box height whenever the
            // child never called `set_baselines` (confirmed against masonry
            // c5950bc's `WidgetState::layout_first_baseline`, which returns
            // `self.layout_border_box_size.height` when `first_baseline.is_nan()`).
            // So a widget with no real baseline reports baseline == height; a real
            // text baseline sits strictly above the bottom edge, i.e. baseline <
            // height. Detect the substitution that way instead of `is_finite()`,
            // which never actually fires.
            let label_has_base = label_base < lh - Self::BASELINE_SENTINEL_EPS;
            let value_has_base = value_base < vs.height - Self::BASELINE_SENTINEL_EPS;
            let (label_dy, value_dy) = if label_has_base && value_has_base {
                // Offset whichever child's baseline sits higher so both baselines
                // land on the same line.
                let target = label_base.max(value_base);
                (target - label_base, target - value_base)
            } else {
                // At least one child has no real text baseline: top-align the row.
                (0.0, 0.0)
            };

            ctx.place_child(label, Point::new(0.0, y + label_dy));
            ctx.place_child(value, Point::new(value_x, y + value_dy));

            let row_h = (label_dy + lh).max(value_dy + vs.height);
            y += row_h + row_gap;
        }
    }

    /// Real stacked layout: each item's value is placed directly below its own
    /// label, both at full width; items stack vertically with `row_gap`
    /// between them.
    fn layout_stacked(&mut self, ctx: &mut LayoutCtx<'_>, size: Size) {
        let full = LayoutSize::from(size);
        let pair_gap = self.pair_gap();
        let row_gap = self.row_gap();
        let mut y = 0.0_f64;
        for i in 0..self.labels.len() {
            let label = &mut self.labels[i];
            let ls = ctx.compute_size(label, SizeDef::MIN, full);
            let lh = ls.height;
            ctx.run_layout(label, Size::new(size.width, lh));
            ctx.place_child(label, Point::new(0.0, y));
            y += lh + pair_gap;

            let value = &mut self.values[i];
            let vs = ctx.compute_size(value, SizeDef::MIN, full);
            ctx.run_layout(value, Size::new(size.width, vs.height));
            ctx.place_child(value, Point::new(0.0, y));
            y += vs.height;

            if i + 1 < self.labels.len() {
                y += row_gap;
            }
        }
    }

    /// Real stacked measurement, mirroring `layout_stacked`: horizontal
    /// preferred width is the widest label or value; vertical preferred
    /// height is the sum of each item's label + `pair_gap` + value, plus
    /// `row_gap` between items.
    fn measure_stacked(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let pair_gap = self.pair_gap();
        let row_gap = self.row_gap();
        let n = self.labels.len();
        // Sizes children via the incoming `len_req`, while `layout_stacked`
        // sizes the same children via `SizeDef::MIN`. The two agree only
        // while values are intrinsically-sized (min-content == fit-content),
        // which holds for the current value set (text/badge/status_dot).
        // Revisit this split if a wrapping/growable value type is introduced.
        match axis {
            Axis::Horizontal => {
                let mut w = 0.0_f64;
                for i in 0..n {
                    let lw = ctx.compute_length(
                        &mut self.labels[i],
                        len_req.into(),
                        context_size,
                        Axis::Horizontal,
                        cross_length,
                    );
                    let vw = ctx.compute_length(
                        &mut self.values[i],
                        len_req.into(),
                        context_size,
                        Axis::Horizontal,
                        cross_length,
                    );
                    w = w.max(lw.get()).max(vw.get());
                }
                Length::px(w)
            }
            Axis::Vertical => {
                let mut h = 0.0_f64;
                for i in 0..n {
                    let lh = ctx.compute_length(
                        &mut self.labels[i],
                        len_req.into(),
                        context_size,
                        Axis::Vertical,
                        cross_length,
                    );
                    let vh = ctx.compute_length(
                        &mut self.values[i],
                        len_req.into(),
                        context_size,
                        Axis::Vertical,
                        cross_length,
                    );
                    h += lh.get() + pair_gap + vh.get();
                    if i + 1 < n {
                        h += row_gap;
                    }
                }
                Length::px(h)
            }
        }
    }
}

// --- MARK: WIDGETMUT

impl DescriptionListWidget {
    pub(super) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
        }
    }

    pub(super) fn set_orientation(this: &mut WidgetMut<'_, Self>, o: DescriptionListOrientation) {
        if this.widget.orientation != o {
            this.widget.orientation = o;
            this.ctx.request_layout();
        }
    }

    /// Replaces the entire label/value child set, e.g. when the item *count*
    /// changes.
    pub(super) fn set_items(
        this: &mut WidgetMut<'_, Self>,
        labels: Vec<NewWidget<Passthrough>>,
        values: Vec<NewWidget<Passthrough>>,
    ) {
        // Drop old children so masonry unregisters them, then register the new set.
        for pod in this.widget.labels.drain(..) {
            this.ctx.remove_child(pod);
        }
        for pod in this.widget.values.drain(..) {
            this.ctx.remove_child(pod);
        }
        this.widget.labels = labels.into_iter().map(NewWidget::to_pod).collect();
        this.widget.values = values.into_iter().map(NewWidget::to_pod).collect();
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Returns a `WidgetMut` for the label at `i`.
    ///
    /// Label and value of item `i` are built under the same `ViewId::new(i)`
    /// (see `view.rs`'s `build`/`rebuild`/`message`) — safe today because
    /// labels are static, non-interactive `Label`s that never route a
    /// message. If a label ever becomes interactive, split the id space
    /// (`2*i` label, `2*i+1` value) rather than reusing this shared index.
    pub(super) fn label_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        i: usize,
    ) -> WidgetMut<'t, Passthrough> {
        this.ctx.get_mut(&mut this.widget.labels[i])
    }

    pub(super) fn value_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        i: usize,
    ) -> WidgetMut<'t, Passthrough> {
        this.ctx.get_mut(&mut this.widget.values[i])
    }
}

// --- MARK: IMPL WIDGET

impl Widget for DescriptionListWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Interleave label/value per item (label_0, value_0, label_1, …) so
        // registration order mirrors visual row grouping and matches
        // `children_ids`.
        for (label, value) in self.labels.iter_mut().zip(self.values.iter_mut()) {
            ctx.register_child(label);
            ctx.register_child(value);
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _type_id: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        match self.orientation {
            DescriptionListOrientation::Horizontal => {
                let n = self.labels.len();
                let column_gap = self.column_gap();
                let row_gap = self.row_gap();

                // Shared label-column width: widest label's intrinsic width.
                // Needed by both axes — the width arm returns it directly, the
                // height arm subtracts it to reproduce `layout_horizontal`'s
                // `value_avail` value width.
                let mut col_w = 0.0_f64;
                for i in 0..n {
                    let lw = ctx.compute_length(
                        &mut self.labels[i],
                        len_req.into(),
                        context_size,
                        Axis::Horizontal,
                        cross_length,
                    );
                    col_w = col_w.max(lw.get());
                }

                match axis {
                    Axis::Horizontal => {
                        // Intrinsic total width: label column + gap + widest
                        // value at its own (unconstrained) preferred width.
                        let mut value_w = 0.0_f64;
                        for i in 0..n {
                            let vw = ctx.compute_length(
                                &mut self.values[i],
                                len_req.into(),
                                context_size,
                                Axis::Horizontal,
                                cross_length,
                            );
                            value_w = value_w.max(vw.get());
                        }
                        Length::px(col_w + column_gap + value_w)
                    }
                    Axis::Vertical => {
                        // Height must mirror `layout_horizontal`, which lays each
                        // value out constrained to `value_avail` — the width left
                        // past the label column. Measuring the value's height at
                        // that same narrower width lets a wrapping value report
                        // its true (taller, wrapped) height, so the parent
                        // allocates enough room and the value doesn't overflow /
                        // clip. Without a known width (`cross_length == None`,
                        // i.e. the parent is asking for intrinsic height) fall
                        // back to the value's own unconstrained height.
                        //
                        // Remaining `measure`/`layout` gap: this arm still uses
                        // the incoming `len_req` as the value's auto-length while
                        // `layout_horizontal` uses `SizeDef::MIN`. Those agree for
                        // intrinsically-sized values (min-content == fit-content).
                        let value_avail = cross_length
                            .map(|w| Length::px((w.get() - col_w - column_gap).max(0.0)));
                        let mut total_h = 0.0_f64;
                        for i in 0..n {
                            let lh = ctx.compute_length(
                                &mut self.labels[i],
                                len_req.into(),
                                context_size,
                                Axis::Vertical,
                                cross_length,
                            );
                            let vh = ctx.compute_length(
                                &mut self.values[i],
                                len_req.into(),
                                context_size,
                                Axis::Vertical,
                                value_avail.or(cross_length),
                            );
                            // Simplification: approximate row height as
                            // max(label_h, value_h), ignoring the baseline-
                            // alignment offsets that `layout_horizontal` applies
                            // to actual placement. When a row's value has an
                            // atypical baseline (rare — labels are fixed caption
                            // text and typical values are text too), this can
                            // slightly under-report the height `layout` actually
                            // produces. Accepted as a known, intentional gap
                            // rather than running a second layout pass inside
                            // `measure`.
                            total_h += lh.get().max(vh.get());
                            if i + 1 < n {
                                total_h += row_gap;
                            }
                        }
                        Length::px(total_h)
                    }
                }
            }
            DescriptionListOrientation::Stacked => {
                self.measure_stacked(ctx, axis, len_req, cross_length)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        match self.orientation {
            DescriptionListOrientation::Horizontal => self.layout_horizontal(ctx, size),
            DescriptionListOrientation::Stacked => self.layout_stacked(ctx, size),
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self
            .labels
            .iter()
            .zip(self.values.iter())
            .flat_map(|(label, value)| [label.id(), value.id()])
            .collect();
        ChildrenIds::from_slice(&ids)
    }
}

// --- MARK: TESTS

#[cfg(test)]
mod tests {
    use masonry::core::{NewWidget, Widget};
    use masonry::layout::Length;
    use masonry::testing::TestHarness;
    use masonry::widgets::{Label, Passthrough, SizedBox};

    use super::DescriptionListWidget;
    use crate::Theme;
    use crate::components::description_list::view::DescriptionListOrientation;

    fn passthrough(text: &str) -> NewWidget<Passthrough> {
        NewWidget::new(Passthrough::new(NewWidget::new(Label::new(text)).erased()))
    }

    /// A plain `SizedBox` never calls `ctx.set_baselines`, so it has no real
    /// text baseline — masonry substitutes its own border-box height as its
    /// "baseline" (see `layout_horizontal`'s `*_has_base` comment). Used to
    /// exercise the top-align fallback when a value isn't text.
    /// A text value that word-wraps under a width constraint (masonry's default
    /// `LineBreaking` is `Overflow`, which never wraps). Used to exercise the
    /// wrapping-value sizing contract between `measure` and `layout_horizontal`.
    fn passthrough_wrap(text: &str) -> NewWidget<Passthrough> {
        use masonry::core::PropertySet;
        use masonry::properties::LineBreaking;
        let label = NewWidget::new(Label::new(text))
            .with_props(PropertySet::new().with(LineBreaking::WordWrap));
        NewWidget::new(Passthrough::new(label.erased()))
    }

    fn passthrough_box(width: f64, height: f64) -> NewWidget<Passthrough> {
        NewWidget::new(Passthrough::new(
            NewWidget::new(
                SizedBox::empty()
                    .width(Length::px(width))
                    .height(Length::px(height)),
            )
            .erased(),
        ))
    }

    fn widget() -> DescriptionListWidget {
        DescriptionListWidget::new(
            vec![passthrough("Name"), passthrough("Role")],
            vec![passthrough("Ada"), passthrough("Mathematician")],
            DescriptionListOrientation::Horizontal,
            &Theme::default(),
        )
    }

    #[test]
    fn children_ids_interleave_label_then_value_per_item() {
        let mut h = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(widget()),
        );
        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.labels.len(), 2);
            assert_eq!(wm.widget.values.len(), 2);
            let expected = vec![
                wm.widget.labels[0].id(),
                wm.widget.values[0].id(),
                wm.widget.labels[1].id(),
                wm.widget.values[1].id(),
            ];
            let actual: Vec<_> = wm.widget.children_ids().iter().copied().collect();
            assert_eq!(actual, expected);
        });
    }

    #[test]
    fn horizontal_values_share_a_common_x_past_the_widest_label() {
        // Labels of very different widths; values must still align.
        let w = DescriptionListWidget::new(
            vec![passthrough("A"), passthrough("A much longer label")],
            vec![passthrough("first value"), passthrough("second value")],
            DescriptionListOrientation::Horizontal,
            &Theme::default(),
        );
        let mut h = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(w),
            (600, 200),
        );

        // Read each value child's window-space left edge.
        let value_ids: Vec<_> = h.edit_root_widget(|wm| {
            wm.widget
                .values
                .iter()
                .map(masonry::core::WidgetPod::id)
                .collect::<Vec<_>>()
        });
        let x0 = h
            .get_widget_with_id(value_ids[0])
            .ctx()
            .bounding_box()
            .min_x();
        let x1 = h
            .get_widget_with_id(value_ids[1])
            .ctx()
            .bounding_box()
            .min_x();
        assert!(
            (x0 - x1).abs() < 0.5,
            "values not column-aligned: {x0} vs {x1}"
        );
        // And the column starts past the short label's own width.
        assert!(x0 > 20.0, "value column should sit past the widest label");
    }

    #[test]
    fn horizontal_top_aligns_row_when_value_has_no_text_baseline() {
        // The value is a plain SizedBox (no `set_baselines` call), so it has no
        // real text baseline — `layout_horizontal` must fall back to top-align
        // rather than baseline-align this row (which would otherwise pin the
        // box's bottom edge to the label's baseline).
        let w = DescriptionListWidget::new(
            vec![passthrough("Status")],
            vec![passthrough_box(40.0, 8.0)],
            DescriptionListOrientation::Horizontal,
            &Theme::default(),
        );
        let mut h = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(w),
            (600, 200),
        );

        let (label_id, value_id) = h.edit_root_widget(|wm| {
            (
                masonry::core::WidgetPod::id(&wm.widget.labels[0]),
                masonry::core::WidgetPod::id(&wm.widget.values[0]),
            )
        });
        let label_min_y = h.get_widget_with_id(label_id).ctx().bounding_box().min_y();
        let value_min_y = h.get_widget_with_id(value_id).ctx().bounding_box().min_y();
        assert!(
            (label_min_y - value_min_y).abs() < 0.5,
            "row should top-align when the value has no text baseline: label min_y \
             {label_min_y} vs value min_y {value_min_y}"
        );
    }

    #[test]
    fn horizontal_measured_height_covers_wrapped_value() {
        use masonry::widgets::Flex;

        // Regression: `measure`'s Horizontal arm must size a wrapping value's
        // height at the same narrow width (`value_avail`) that
        // `layout_horizontal` lays it out at. If `measure` sizes the value at
        // the full parent width instead, it under-reports the wrapped height,
        // the parent lays the taller value out anyway, and it overflows the box
        // the parent allocated from that measurement — clipping / overlapping
        // whatever sits below.
        //
        // Structure that makes the under-report observable:
        //   Flex column [ list, marker ]   in a 120px-wide window
        // A `Flex` column measures each child's height passing its own available
        // width as `cross_length` and stacks children at that measured height.
        // Pinning the window to 120px makes that measurement width equal the
        // width the list is later laid out at, so `measure` and
        // `layout_horizontal` see the same narrow `value_avail` and the value
        // genuinely wraps in both. If `measure` sized the value at the full
        // window width instead of `value_avail`, it would under-report the
        // wrapped height, the marker would sit too high, and the taller
        // laid-out value would overlap it.
        let value = passthrough_wrap(
            "the quick brown fox jumps over the lazy dog again and again and again",
        );
        let value_id = value.id();
        let list = NewWidget::new(DescriptionListWidget::new(
            vec![passthrough("Description")],
            vec![value],
            DescriptionListOrientation::Horizontal,
            &Theme::default(),
        ));
        let marker = passthrough_box(10.0, 6.0);
        let marker_id = marker.id();

        let column = NewWidget::new(
            Flex::column()
                .with_fixed(list.erased())
                .with_fixed(marker.erased()),
        );
        let h = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            column,
            (120, 400),
        );

        let value_max_y = h.get_widget_with_id(value_id).ctx().bounding_box().max_y();
        let marker_min_y = h.get_widget_with_id(marker_id).ctx().bounding_box().min_y();
        assert!(
            value_max_y <= marker_min_y + 0.5,
            "measured height under-reports wrapped value: value bottom {value_max_y} \
             overlaps the sibling below it at {marker_min_y}"
        );
    }

    #[test]
    fn stacked_places_each_value_below_its_label() {
        let w = DescriptionListWidget::new(
            vec![passthrough("Name")],
            vec![passthrough("Ada Lovelace")],
            DescriptionListOrientation::Stacked,
            &Theme::default(),
        );
        let mut h = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(w),
            (400, 200),
        );
        let (lid, vid) = h.edit_root_widget(|wm| {
            (
                masonry::core::WidgetPod::id(&wm.widget.labels[0]),
                masonry::core::WidgetPod::id(&wm.widget.values[0]),
            )
        });
        let label_bottom = h.get_widget_with_id(lid).ctx().bounding_box().max_y();
        let value_top = h.get_widget_with_id(vid).ctx().bounding_box().min_y();
        assert!(
            value_top >= label_bottom - 0.5,
            "value should sit below its label"
        );
    }
}
