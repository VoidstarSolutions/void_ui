//! Xilem view layer for the context menu's rich item panel.
//!
//! [`menu`] starts a builder; [`item`] builds a single command row. Items carry
//! their `on_select` callback, separators are added with [`Menu::separator`],
//! and [`Menu::render`] materializes a [`MenuView`] that drives the
//! [`MenuPanel`](super::widget::MenuPanel) widget. Selecting an enabled row
//! fires that row's callback.
//!
//! ```ignore
//! use void_ui::components::context_menu::{menu, item};
//! menu()
//!     .item(item("Copy").on_select(|s: &mut State| s.copy()))
//!     .item(item("Paste").disabled(true).on_select(|s: &mut State| s.paste()))
//!     .separator()
//!     .item(item("Select All").on_select(|s: &mut State| s.select_all()))
//!     .render(&theme)
//! ```
//!
//! This is the foundation surface (chunk 1): flat actions, separators, disabled
//! rows. The right-click trigger and richer columns (icon/shortcut/check,
//! submenus) build on top in later chunks.

use std::marker::PhantomData;

use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::{MenuItemSelected, MenuPanel, MenuRowSpec};
use crate::Theme;

type SelectCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// A single command row of a [`Menu`], before it is added via [`Menu::item`].
///
/// Build one with [`item`]; chain [`Self::disabled`] / [`Self::on_select`].
#[must_use = "MenuItem does nothing until added to a menu() with .item(...)"]
pub struct MenuItem<State, Action> {
    label: ArcStr,
    disabled: bool,
    on_select: Option<SelectCallback<State, Action>>,
}

/// Start building a menu command row with the given label.
pub fn item<State, Action>(label: impl Into<ArcStr>) -> MenuItem<State, Action> {
    MenuItem {
        label: label.into(),
        disabled: false,
        on_select: None,
    }
}

impl<State, Action> MenuItem<State, Action> {
    /// Mute the row and block its selection.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the callback fired when this row is selected.
    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        self.on_select = Some(Box::new(callback));
        self
    }
}

/// One entry in a menu: a command row or a divider.
enum Entry<State, Action> {
    Item(MenuItem<State, Action>),
    Separator,
}

/// Builder for a rich menu panel.
///
/// Create with [`menu`]; add rows with [`Self::item`] / [`Self::separator`];
/// materialize with [`Self::render`].
#[must_use = "Menu does nothing until rendered with .render(&theme)"]
pub struct Menu<State, Action> {
    entries: Vec<Entry<State, Action>>,
}

/// Start building an empty menu.
pub fn menu<State, Action>() -> Menu<State, Action> {
    Menu {
        entries: Vec::new(),
    }
}

impl<State, Action> Default for Menu<State, Action> {
    fn default() -> Self {
        menu()
    }
}

impl<State, Action> Menu<State, Action>
where
    State: 'static,
    Action: 'static,
{
    /// Append a command row.
    pub fn item(mut self, item: MenuItem<State, Action>) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    /// Append a divider between rows.
    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> MenuView<State, Action> {
        let mut rows = Vec::with_capacity(self.entries.len());
        let mut callbacks = Vec::with_capacity(self.entries.len());
        for entry in self.entries {
            match entry {
                Entry::Item(it) => {
                    rows.push(MenuRowSpec::Action {
                        label: it.label,
                        disabled: it.disabled,
                    });
                    // Disabled rows never emit a selection, so their callback is
                    // irrelevant — drop it to keep the index→callback mapping honest.
                    callbacks.push(if it.disabled { None } else { it.on_select });
                }
                Entry::Separator => {
                    rows.push(MenuRowSpec::Separator);
                    callbacks.push(None);
                }
            }
        }
        MenuView {
            rows,
            callbacks,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`Menu`].
///
/// Not constructed directly; use [`Menu::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct MenuView<State, Action> {
    rows: Vec<MenuRowSpec>,
    callbacks: Vec<Option<SelectCallback<State, Action>>>,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for MenuView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for MenuView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<MenuPanel>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = MenuPanel::new(self.rows.iter().cloned(), &self.theme);
        // Register as an action source so the widget's `MenuItemSelected`
        // bubbles to this view's `message` rather than an ancestor widget.
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
            MenuPanel::set_theme(&mut element, &self.theme);
        }
        if self.rows != prev.rows {
            MenuPanel::set_rows(&mut element, self.rows.iter().cloned());
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
        match message.take_message::<MenuItemSelected>() {
            Some(selected) => {
                let MenuItemSelected(index) = *selected;
                match self.callbacks.get(index) {
                    Some(Some(callback)) => MessageResult::Action(callback(app_state)),
                    // Selected an enabled row that carries no callback — consumed,
                    // but there's nothing to emit.
                    Some(None) => MessageResult::Nop,
                    None => MessageResult::Stale,
                }
            }
            None => MessageResult::Stale,
        }
    }
}
