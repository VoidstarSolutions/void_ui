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
//! Rows can be actions (with icon/shortcut/checked/sub-title/disabled),
//! separators, section headers, or [`submenu`]s (hover-open fly-outs of nested
//! entries). Each command is assigned a stable leaf id so selections — at any
//! nesting depth — route back to the right callback.

use std::marker::PhantomData;

use masonry::core::{ArcStr, NewWidget};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::area::{ContextMenuAction, ContextMenuArea};
use super::widget::{MenuAction, MenuPanel, MenuRowSpec};
use crate::Theme;
use crate::components::icon::IconName;

type SelectCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// A single command row of a [`Menu`], before it is added via [`Menu::item`].
///
/// Build one with [`item`]; chain [`Self::disabled`] / [`Self::on_select`].
#[must_use = "MenuItem does nothing until added to a menu() with .item(...)"]
pub struct MenuItem<State, Action> {
    label: ArcStr,
    subtitle: Option<ArcStr>,
    icon: Option<IconName>,
    shortcut: Option<ArcStr>,
    checked: Option<bool>,
    disabled: bool,
    on_select: Option<SelectCallback<State, Action>>,
}

/// Start building a menu command row with the given label.
pub fn item<State, Action>(label: impl Into<ArcStr>) -> MenuItem<State, Action> {
    MenuItem {
        label: label.into(),
        subtitle: None,
        icon: None,
        shortcut: None,
        checked: None,
        disabled: false,
        on_select: None,
    }
}

impl<State, Action> MenuItem<State, Action> {
    /// Attach a secondary line of text under the label.
    pub fn subtitle(mut self, subtitle: impl Into<ArcStr>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Attach a leading icon from the Lucide icon set. Ignored when the row is
    /// also [`checkable`](Self::checked) — the check takes the gutter.
    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

    /// Attach trailing keyboard-shortcut text (e.g. `"Ctrl+C"`).
    pub fn shortcut(mut self, shortcut: impl Into<ArcStr>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Make this a checkable row, showing a check in the gutter when `checked`.
    /// Checkable rows reserve the gutter even when unchecked so labels align.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

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

/// Builder for a submenu — a labeled row that opens a nested fly-out of its
/// own items on hover. Build with [`submenu`], add rows the same way as a
/// [`Menu`] (including nested [`Self::submenu`]s), then add it to a parent with
/// `.submenu(...)`.
#[must_use = "Submenu does nothing until added to a menu with .submenu(...)"]
pub struct Submenu<State, Action> {
    label: ArcStr,
    icon: Option<IconName>,
    entries: Vec<Entry<State, Action>>,
}

/// Start building a submenu with the given label.
pub fn submenu<State, Action>(label: impl Into<ArcStr>) -> Submenu<State, Action> {
    Submenu {
        label: label.into(),
        icon: None,
        entries: Vec::new(),
    }
}

impl<State, Action> Submenu<State, Action> {
    /// Attach a leading icon from the Lucide icon set.
    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

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

    /// Append a non-interactive, muted section header.
    pub fn section(mut self, text: impl Into<ArcStr>) -> Self {
        self.entries.push(Entry::Section(text.into()));
        self
    }

    /// Append a nested submenu.
    pub fn submenu(mut self, submenu: Submenu<State, Action>) -> Self {
        self.entries.push(submenu.into_entry());
        self
    }

    fn into_entry(self) -> Entry<State, Action> {
        Entry::Submenu {
            label: self.label,
            icon: self.icon,
            children: self.entries,
        }
    }
}

/// One entry in a menu: a command row, a divider, a section header, or a
/// submenu (a labeled fly-out of nested entries).
enum Entry<State, Action> {
    Item(MenuItem<State, Action>),
    Separator,
    Section(ArcStr),
    Submenu {
        label: ArcStr,
        icon: Option<IconName>,
        children: Vec<Entry<State, Action>>,
    },
}

/// Recursively translate builder entries into the widget's display specs,
/// assigning each command row a stable global leaf id and pushing its callback
/// at that id into `callbacks` (so `callbacks[id]` is the row's callback,
/// across any nesting). Shared by the inline [`Menu`] and the
/// [`context_menu_area`] trigger.
fn entries_to_rows<State, Action>(
    entries: Vec<Entry<State, Action>>,
    next_id: &mut usize,
    callbacks: &mut Vec<Option<SelectCallback<State, Action>>>,
) -> Vec<MenuRowSpec> {
    entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Item(it) => {
                let id = *next_id;
                *next_id += 1;
                // Ids are allocated in lockstep with this push, so the callback
                // lands at index `id`. Disabled rows never fire, so drop theirs.
                debug_assert_eq!(callbacks.len(), id);
                callbacks.push(if it.disabled { None } else { it.on_select });
                MenuRowSpec::Action {
                    id,
                    label: it.label,
                    subtitle: it.subtitle,
                    icon: it.icon,
                    shortcut: it.shortcut,
                    checked: it.checked,
                    disabled: it.disabled,
                }
            }
            Entry::Separator => MenuRowSpec::Separator,
            Entry::Section(text) => MenuRowSpec::Section { text },
            Entry::Submenu {
                label,
                icon,
                children,
            } => MenuRowSpec::Submenu {
                label,
                icon,
                children: entries_to_rows(children, next_id, callbacks),
            },
        })
        .collect()
}

/// Build the top-level spec list and the id→callback table from builder entries.
fn build_menu<State, Action>(
    entries: Vec<Entry<State, Action>>,
) -> (Vec<MenuRowSpec>, Vec<Option<SelectCallback<State, Action>>>) {
    let mut next_id = 0;
    let mut callbacks = Vec::new();
    let rows = entries_to_rows(entries, &mut next_id, &mut callbacks);
    (rows, callbacks)
}

/// Route a selected leaf id to its callback, shared by both consumers.
fn dispatch_selection<State, Action>(
    callbacks: &[Option<SelectCallback<State, Action>>],
    id: usize,
    app_state: &mut State,
) -> MessageResult<Action> {
    match callbacks.get(id) {
        Some(Some(callback)) => MessageResult::Action(callback(app_state)),
        // Selected an enabled row that carries no callback — consumed, but
        // there's nothing to emit.
        Some(None) => MessageResult::Nop,
        None => MessageResult::Stale,
    }
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

    /// Append a non-interactive, muted section header.
    pub fn section(mut self, text: impl Into<ArcStr>) -> Self {
        self.entries.push(Entry::Section(text.into()));
        self
    }

    /// Append a submenu that opens a nested fly-out on hover.
    pub fn submenu(mut self, submenu: Submenu<State, Action>) -> Self {
        self.entries.push(submenu.into_entry());
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> MenuView<State, Action> {
        let (rows, callbacks) = build_menu(self.entries);
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
        match message.take_message::<MenuAction>().map(|a| *a) {
            Some(MenuAction::Selected(index)) => {
                dispatch_selection(&self.callbacks, index, app_state)
            }
            // A standalone inline menu has nothing to dismiss — consume it.
            Some(MenuAction::Dismissed) => MessageResult::Nop,
            None => MessageResult::Stale,
        }
    }
}

// --- MARK: context_menu_area (right-click trigger)

/// Builder for a right-click context menu over arbitrary `content`.
///
/// Create with [`context_menu_area`]; add rows with [`Self::item`] /
/// [`Self::separator`]; materialize with [`Self::render`]. Right-clicking
/// anywhere in `content` opens the menu at the cursor; selecting a row fires its
/// callback, and selecting / clicking outside / Escape dismisses it.
///
/// ```ignore
/// use void_ui::components::context_menu::{context_menu_area, item};
/// context_menu_area(my_content.render(&theme))
///     .item(item("Cut").on_select(|s: &mut State| s.cut()))
///     .item(item("Copy").on_select(|s: &mut State| s.copy()))
///     .separator()
///     .item(item("Paste").disabled(true).on_select(|s: &mut State| s.paste()))
///     .render(&theme)
/// ```
#[must_use = "ContextMenuAreaBuilder does nothing until rendered with .render(&theme)"]
pub struct ContextMenuAreaBuilder<State, Action, V> {
    content: V,
    entries: Vec<Entry<State, Action>>,
}

/// Wrap `content` so a right-click opens a context menu over it.
pub fn context_menu_area<State, Action, V>(content: V) -> ContextMenuAreaBuilder<State, Action, V> {
    ContextMenuAreaBuilder {
        content,
        entries: Vec::new(),
    }
}

impl<State, Action, V> ContextMenuAreaBuilder<State, Action, V>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
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

    /// Append a non-interactive, muted section header.
    pub fn section(mut self, text: impl Into<ArcStr>) -> Self {
        self.entries.push(Entry::Section(text.into()));
        self
    }

    /// Append a submenu that opens a nested fly-out on hover.
    pub fn submenu(mut self, submenu: Submenu<State, Action>) -> Self {
        self.entries.push(submenu.into_entry());
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> ContextMenuAreaView<V, State, Action> {
        let (rows, callbacks) = build_menu(self.entries);
        ContextMenuAreaView {
            content: self.content,
            rows,
            callbacks,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`ContextMenuAreaBuilder`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ContextMenuAreaView<V, State, Action> {
    content: V,
    rows: Vec<MenuRowSpec>,
    callbacks: Vec<Option<SelectCallback<State, Action>>>,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for ContextMenuAreaView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for ContextMenuAreaView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<ContextMenuArea>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (content_pod, content_vs) = self.content.build(ctx, app_state);
        // Host-driven: the area owns focus and forwards keys, so the menu
        // doesn't grab focus on click.
        let menu = MenuPanel::new(self.rows.iter().cloned(), &self.theme).hosted();
        let widget = ContextMenuArea::new(content_pod.new_widget.erased(), NewWidget::new(menu));
        // Register as an action source so the area's `ContextMenuAction` routes
        // to this view's `message`.
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, content_vs)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        {
            let mut content = ContextMenuArea::content_mut(&mut element);
            self.content.rebuild(
                &prev.content,
                view_state,
                ctx,
                content.downcast(),
                app_state,
            );
        }
        if self.theme != prev.theme {
            let mut menu = ContextMenuArea::menu_mut(&mut element);
            MenuPanel::set_theme(&mut menu, &self.theme);
        }
        if self.rows != prev.rows {
            let mut menu = ContextMenuArea::menu_mut(&mut element);
            MenuPanel::set_rows(&mut menu, self.rows.iter().cloned());
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut content = ContextMenuArea::content_mut(&mut element);
            self.content.teardown(view_state, ctx, content.downcast());
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if let Some(action) = message.take_message::<ContextMenuAction>() {
            let ContextMenuAction::ItemSelected(index) = *action;
            dispatch_selection(&self.callbacks, index, app_state)
        } else {
            let mut content = ContextMenuArea::content_mut(&mut element);
            self.content
                .message(view_state, message, content.downcast(), app_state)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_menu_assigns_sequential_leaf_ids_across_nesting() {
        // a, [submenu S: b, c], d → leaf ids 0,1,2,3 in tree order; the
        // callbacks table has one slot per command, indexed by id.
        let entries: Vec<Entry<(), ()>> = vec![
            Entry::Item(item("a")),
            submenu::<(), ()>("S")
                .item(item("b"))
                .item(item("c"))
                .into_entry(),
            Entry::Item(item("d")),
        ];
        let (rows, callbacks) = build_menu(entries);

        assert_eq!(callbacks.len(), 4);
        assert!(matches!(rows[0], MenuRowSpec::Action { id: 0, .. }));
        assert!(matches!(rows[2], MenuRowSpec::Action { id: 3, .. }));
        match &rows[1] {
            MenuRowSpec::Submenu { children, .. } => {
                assert!(matches!(children[0], MenuRowSpec::Action { id: 1, .. }));
                assert!(matches!(children[1], MenuRowSpec::Action { id: 2, .. }));
            }
            _ => panic!("expected a submenu row"),
        }
    }
}
