//! Xilem view layer for the dropdown button component.
//!
//! `DropdownButton<F, State, Action>` is the builder; `.render(&theme)`
//! produces a `DropdownButtonView` which implements xilem's `View` trait.
//!
//! ```ignore
//! use void_ui::components::dropdown_button;
//! dropdown_button("Save", |s: &mut State| s.save())
//!     .item("Save as…", |s: &mut State| s.save_as())
//!     .item("Export", |s: &mut State| s.export())
//!     .render(&theme)
//! ```

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::core::{ArcStr, StyleProperty};
use masonry::kurbo::BezPath;
use masonry::properties::ContentColor;
use masonry::widgets::Label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::{DropdownButtonAction, ThemedDropdownButton};
use crate::Theme;
use crate::components::button::ButtonVariant;

type ItemCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// Builder for a split dropdown button.
///
/// Create with [`dropdown_button`]; add menu items via [`Self::item`].
/// Materialize as a xilem view via [`Self::render`].
#[must_use = "DropdownButton does nothing until rendered with .render(&theme)"]
pub struct DropdownButton<F, State, Action> {
    label: ArcStr,
    main_callback: F,
    icon: Option<Arc<BezPath>>,
    items: Vec<(ArcStr, ItemCallback<State, Action>)>,
    variant: ButtonVariant,
    disabled: bool,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Construct a dropdown button with the given label and primary-action callback.
///
/// The primary callback fires when the LEFT part of the button is clicked.
/// Attach menu items via [`DropdownButton::item`]; clicking them opens a
/// floating overlay with the item list.
pub fn dropdown_button<F, State, Action>(
    label: impl Into<ArcStr>,
    main_callback: F,
) -> DropdownButton<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    DropdownButton {
        label: label.into(),
        main_callback,
        icon: None,
        items: Vec::new(),
        variant: ButtonVariant::Default,
        disabled: false,
        phantom: PhantomData,
    }
}

impl<F, State, Action> DropdownButton<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    /// Add a menu item that fires `callback` when selected.
    pub fn item<G>(mut self, label: impl Into<ArcStr>, callback: G) -> Self
    where
        G: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        self.items.push((label.into(), Box::new(callback)));
        self
    }

    /// Attach a leading icon (unit-square `BezPath`, scaled to UI font size).
    pub fn icon(mut self, path: BezPath) -> Self {
        self.icon = Some(Arc::new(path));
        self
    }

    /// Set the visual style variant.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DropdownButtonView<F, State, Action> {
        let item_labels: Vec<ArcStr> = self.items.iter().map(|(lbl, _)| lbl.clone()).collect();
        DropdownButtonView {
            label: self.label,
            main_callback: self.main_callback,
            icon: self.icon,
            items: self.items,
            item_labels,
            variant: self.variant,
            disabled: self.disabled,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`DropdownButton`].
///
/// Not constructed directly; use [`DropdownButton::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DropdownButtonView<F, State, Action> {
    label: ArcStr,
    main_callback: F,
    icon: Option<Arc<BezPath>>,
    items: Vec<(ArcStr, ItemCallback<State, Action>)>,
    item_labels: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for DropdownButtonView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for DropdownButtonView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedDropdownButton>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = ThemedDropdownButton::new(
            self.label.clone(),
            self.icon.clone(),
            self.item_labels.clone(),
            self.variant,
            self.disabled,
            &self.theme,
        );
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
            ThemedDropdownButton::set_theme(&mut element, &self.theme);
            // Update label text color for new theme
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else if self.variant == ButtonVariant::Link {
                self.theme.palette.teal
            } else {
                self.theme.palette.text
            };
            let mut lbl = ThemedDropdownButton::label_mut(&mut element);
            lbl.insert_prop(ContentColor::new(text_color));
            let mut lbl = lbl.downcast::<Label>();
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.disabled != prev.disabled {
            ThemedDropdownButton::set_disabled(&mut element, self.disabled);
        }
        if self.variant != prev.variant {
            ThemedDropdownButton::set_variant(&mut element, self.variant);
        }
        let icon_changed = match (&self.icon, &prev.icon) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        };
        if icon_changed {
            ThemedDropdownButton::set_icon(&mut element, self.icon.clone());
        }
        if self.item_labels != prev.item_labels {
            ThemedDropdownButton::set_items(&mut element, self.item_labels.clone());
        }
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
        match message.take_message::<DropdownButtonAction>() {
            Some(action) => match *action {
                DropdownButtonAction::MainPressed => {
                    MessageResult::Action((self.main_callback)(app_state))
                }
                DropdownButtonAction::ItemSelected(i) => {
                    if let Some((_, cb)) = self.items.get(i) {
                        MessageResult::Action(cb(app_state))
                    } else {
                        MessageResult::Stale
                    }
                }
            },
            None => MessageResult::Stale,
        }
    }
}
