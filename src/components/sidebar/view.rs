//! Tessera sidebar navigation item — interactive, theme-driven.
//!
//! Wraps [`super::widget::ThemedSidebarItem`] in a xilem [`View`]. Pointer
//! state (hover, press) is tracked by the masonry widget; the `selected` flag
//! is the host-controlled selected-row state.
//!
//! ```ignore
//! use void_ui::components::sidebar_item;
//! sidebar_item("Charts", |s: &mut State| s.focused = Section::Charts)
//!     .selected(state.focused == Section::Charts)
//!     .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::{ArcStr, StyleProperty, Widget as _};
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::ThemedSidebarItem;
use crate::Theme;

/// Builder for a themed sidebar navigation item.
///
/// Created with [`sidebar_item`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "SidebarItem does nothing until rendered with .render(&theme)"]
pub struct SidebarItem<F> {
    label: ArcStr,
    selected: bool,
    disabled: bool,
    callback: F,
}

/// Create a new sidebar navigation item with the given label and selection
/// callback.
///
/// The callback is invoked on primary-pointer release inside the widget and on
/// Space / Enter while the widget is focused.
pub fn sidebar_item<F>(label: impl Into<ArcStr>, callback: F) -> SidebarItem<F> {
    SidebarItem {
        label: label.into(),
        selected: false,
        disabled: false,
        callback,
    }
}

impl<F> SidebarItem<F> {
    /// Mark this item as the currently-selected nav entry.
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> SidebarItemView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        SidebarItemView {
            label: self.label,
            selected: self.selected,
            disabled: self.disabled,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`SidebarItem`].
///
/// Built only through [`SidebarItem::render`]; not constructed directly by
/// callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SidebarItemView<F, State, Action> {
    label: ArcStr,
    selected: bool,
    disabled: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for SidebarItemView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for SidebarItemView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedSidebarItem>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let text_color = if self.disabled {
            self.theme.palette.text_faint
        } else if self.selected {
            self.theme.palette.text
        } else {
            self.theme.palette.text_muted
        };
        let mut label = Label::new(self.label.clone())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        label.properties.insert(ContentColor::new(text_color));
        let widget = ThemedSidebarItem::new(label, &self.theme)
            .with_selected(self.selected)
            .with_disabled(self.disabled);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            ThemedSidebarItem::set_theme(&mut element, &self.theme);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else if self.selected {
                self.theme.palette.text
            } else {
                self.theme.palette.text_muted
            };
            let mut child = ThemedSidebarItem::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
            let mut lbl = child.downcast::<Label>();
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.selected != prev.selected {
            ThemedSidebarItem::set_selected(&mut element, self.selected);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else if self.selected {
                self.theme.palette.text
            } else {
                self.theme.palette.text_muted
            };
            let mut child = ThemedSidebarItem::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
        }
        if self.disabled != prev.disabled {
            ThemedSidebarItem::set_disabled(&mut element, self.disabled);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else if self.selected {
                self.theme.palette.text
            } else {
                self.theme.palette.text_muted
            };
            let mut child = ThemedSidebarItem::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
        }
        // Label text is not re-applied here; masonry's Label doesn't expose
        // stable post-construction text mutation. If the label string changes
        // the parent view should replace the whole item.
        let _ = &prev.label;
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<ButtonPress>() {
            Some(_press) => MessageResult::Action((self.callback)(app_state)),
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sidebar_item;
    use crate::Theme;

    #[test]
    fn selected_is_the_canonical_builder_name() {
        let theme = Theme::default();
        let _ = sidebar_item("Charts", |_: &mut u8| {})
            .selected(true)
            .render::<u8, ()>(&theme);
    }
}
