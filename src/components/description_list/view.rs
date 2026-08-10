//! Xilem view for the description list component.

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::widget::DescriptionListWidget;
use crate::Theme;

/// Layout orientation for a [`DescriptionList`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptionListOrientation {
    /// Label and value side by side; values align in a shared column.
    Horizontal,
    /// Value stacked directly below its label.
    Stacked,
}

/// The same erased view-state xilem uses for `Box<AnyWidgetView<State, Action>>`.
type ItemViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// Builder for a themed list of label/value pairs.
///
/// Created with [`description_list`]. Returns a view via [`Self::render`].
#[must_use = "DescriptionList does nothing until rendered with .render(&theme)"]
pub struct DescriptionList<State, Action = ()> {
    items: Vec<(String, Box<AnyWidgetView<State, Action>>)>,
    orientation: DescriptionListOrientation,
}

/// Create an empty horizontal description list.
pub fn description_list<State, Action>() -> DescriptionList<State, Action> {
    DescriptionList {
        items: Vec::new(),
        orientation: DescriptionListOrientation::Horizontal,
    }
}

impl<State: 'static, Action: 'static> DescriptionList<State, Action> {
    /// Append a label/value pair. `label` is plain text; `value` is any view.
    pub fn item<V>(mut self, label: impl Into<String>, value: V) -> Self
    where
        V: WidgetView<State, Action>,
    {
        self.items.push((label.into(), value.boxed()));
        self
    }

    /// Lay values in a shared aligned column beside their labels (default).
    pub fn horizontal(mut self) -> Self {
        self.orientation = DescriptionListOrientation::Horizontal;
        self
    }

    /// Lay each value directly below its label.
    pub fn stacked(mut self) -> Self {
        self.orientation = DescriptionListOrientation::Stacked;
        self
    }

    /// Materialize a view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DescriptionListView<State, Action> {
        DescriptionListView {
            items: self.items,
            orientation: self.orientation,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`DescriptionList`].
///
/// Built only through [`DescriptionList::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DescriptionListView<State, Action> {
    items: Vec<(String, Box<AnyWidgetView<State, Action>>)>,
    orientation: DescriptionListOrientation,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State: 'static, Action: 'static> DescriptionListView<State, Action> {
    /// Build the internal, themed label child view for one item's label string.
    fn label_view(&self, text: &str) -> Box<AnyWidgetView<State, Action>> {
        crate::label(text.to_string())
            .color(self.theme.palette.text_faint)
            .text_size(self.theme.typography.size_caption)
            .render::<State, Action>(&self.theme)
            .boxed()
    }
}

impl<State, Action> ViewMarker for DescriptionListView<State, Action> {}

impl<State: 'static, Action: 'static> View<State, Action, ViewCtx>
    for DescriptionListView<State, Action>
{
    type Element = Pod<DescriptionListWidget>;
    /// One entry per item: (label view-state, value view-state).
    type ViewState = Vec<(ItemViewState<State, Action>, ItemViewState<State, Action>)>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut labels = Vec::with_capacity(self.items.len());
        let mut values = Vec::with_capacity(self.items.len());
        let mut states = Vec::with_capacity(self.items.len());

        for (i, (label_text, value_view)) in self.items.iter().enumerate() {
            let label_view = self.label_view(label_text);
            #[allow(clippy::cast_possible_truncation)]
            let ((lpod, lstate), (vpod, vstate)) = ctx.with_id(ViewId::new(i as u64), |ctx| {
                let lb = label_view.build(ctx, app_state);
                let vb = value_view.build(ctx, app_state);
                (lb, vb)
            });
            labels.push(lpod.new_widget);
            values.push(vpod.new_widget);
            states.push((lstate, vstate));
        }

        let widget = DescriptionListWidget::new(labels, values, self.orientation, &self.theme);
        let element = ctx.create_pod(widget);
        (element, states)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            DescriptionListWidget::set_theme(&mut element, &self.theme);
        }
        if self.orientation != prev.orientation {
            DescriptionListWidget::set_orientation(&mut element, self.orientation);
        }

        if self.items.len() == prev.items.len() {
            for (i, ((label_text, value_view), (prev_label, prev_value))) in
                self.items.iter().zip(&prev.items).enumerate()
            {
                let label_view = self.label_view(label_text);
                let prev_label_view = prev.label_view(prev_label);
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let (lstate, vstate) = &mut view_state[i];
                    {
                        let mut child = DescriptionListWidget::label_mut(&mut element, i);
                        label_view.rebuild(
                            &prev_label_view,
                            lstate,
                            ctx,
                            child.downcast(),
                            app_state,
                        );
                    }
                    {
                        let mut child = DescriptionListWidget::value_mut(&mut element, i);
                        value_view.rebuild(prev_value, vstate, ctx, child.downcast(), app_state);
                    }
                });
            }
        } else {
            // Count changed: tear down every old child, rebuild the whole set.
            for (i, (prev_label, prev_value)) in prev.items.iter().enumerate() {
                let prev_label_view = prev.label_view(prev_label);
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let (lstate, vstate) = &mut view_state[i];
                    {
                        let mut child = DescriptionListWidget::label_mut(&mut element, i);
                        prev_label_view.teardown(lstate, ctx, child.downcast());
                    }
                    {
                        let mut child = DescriptionListWidget::value_mut(&mut element, i);
                        prev_value.teardown(vstate, ctx, child.downcast());
                    }
                });
            }

            let mut labels = Vec::with_capacity(self.items.len());
            let mut values = Vec::with_capacity(self.items.len());
            let mut new_states = Vec::with_capacity(self.items.len());
            for (i, (label_text, value_view)) in self.items.iter().enumerate() {
                let label_view = self.label_view(label_text);
                #[allow(clippy::cast_possible_truncation)]
                let ((lpod, lstate), (vpod, vstate)) = ctx.with_id(ViewId::new(i as u64), |ctx| {
                    (
                        label_view.build(ctx, app_state),
                        value_view.build(ctx, app_state),
                    )
                });
                labels.push(lpod.new_widget);
                values.push(vpod.new_widget);
                new_states.push((lstate, vstate));
            }
            DescriptionListWidget::set_items(&mut element, labels, values);
            *view_state = new_states;
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        for (i, (label_text, value_view)) in self.items.iter().enumerate() {
            let label_view = self.label_view(label_text);
            #[allow(clippy::cast_possible_truncation)]
            ctx.with_id(ViewId::new(i as u64), |ctx| {
                let (lstate, vstate) = &mut view_state[i];
                {
                    let mut child = DescriptionListWidget::label_mut(&mut element, i);
                    label_view.teardown(lstate, ctx, child.downcast());
                }
                {
                    let mut child = DescriptionListWidget::value_mut(&mut element, i);
                    value_view.teardown(vstate, ctx, child.downcast());
                }
            });
        }
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if message.remaining_path().is_empty() {
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        let index = usize::try_from(id.routing_id()).unwrap_or(usize::MAX);
        if self.items.get(index).is_none() {
            return MessageResult::Stale;
        }
        // Only value children are interactive; route to the value view. Its own
        // internal view-path disambiguates label vs value beneath this ViewId.
        // Both label and value build under the same `ViewId::new(i)` (see
        // `build`/`rebuild` above) — safe only because labels are static
        // `Label`s that never send a message. If a label ever becomes
        // interactive, split the id space (`2*i` label, `2*i+1` value).
        let (_, value_view) = &self.items[index];
        let (_, vstate) = &mut view_state[index];
        let mut child = DescriptionListWidget::value_mut(&mut element, index);
        value_view.message(vstate, message, child.downcast(), app_state)
    }
}

#[cfg(test)]
mod tests {
    use masonry::testing::TestHarness;
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{DescriptionListOrientation, description_list};
    use crate::{Theme, label, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn defaults_to_horizontal_with_no_items() {
        let dl = description_list::<AppState, ()>();
        assert_eq!(dl.orientation, DescriptionListOrientation::Horizontal);
        assert_eq!(dl.items.len(), 0);
    }

    #[test]
    fn item_and_stacked_set_the_expected_fields() {
        let theme = Theme::default();
        let dl = description_list::<AppState, ()>()
            .item("Name", label("Ada").render(&theme))
            .item("Role", label("Mathematician").render(&theme))
            .stacked();
        assert_eq!(dl.orientation, DescriptionListOrientation::Stacked);
        assert_eq!(dl.items.len(), 2);
        assert_eq!(dl.items[0].0, "Name");
        assert_eq!(dl.items[1].0, "Role");
    }

    #[test]
    fn builds_and_rebuilds_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let v0 = description_list::<AppState, ()>()
            .item("Name", label("Ada").render(&theme))
            .render(&theme);
        let (element, mut vs) = v0.build(&mut ctx, &mut state);

        // Mount the built widget in a real tree so a `WidgetMut` can be
        // obtained for `rebuild` — `Pod` has no standalone `as_mut`; a
        // `Mut<'_, Pod<W>>` only exists inside a widget tree (see
        // `xilem_masonry::MasonryRoot::rebuild`, which does the same via
        // `RenderRoot::edit_base_layer`).
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), element.new_widget);

        // Rebuild with an added item exercises the count-change path.
        let v1 = description_list::<AppState, ()>()
            .item("Name", label("Ada").render(&theme))
            .item("Role", label("Mathematician").render(&theme))
            .render(&theme);
        harness.edit_root_widget(|root| {
            v1.rebuild(&v0, &mut vs, &mut ctx, root, &mut state);
        });
    }
}
