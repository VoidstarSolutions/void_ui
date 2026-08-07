//! Themed widget components built on the xilem/masonry primitives.
//!
//! Each component is a small builder that resolves to a `WidgetView` at the
//! supplied [`Theme`](crate::Theme). The pattern is intentionally explicit:
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! use void_ui::components::button;
//! button(|_: &mut State| {}).label("Reset view").render(&theme)
//! # ;
//! ```
//!
//! Theme is passed at the render boundary rather than stored on each
//! component value so that swapping themes is a single state change in
//! the host, not a tree walk.
//!
//! ## Export surface (issue #164)
//!
//! Every component follows the same rule for what reaches this module
//! (and from here, the crate root):
//!
//! 1. The builder and its materialized `View` type (`Foo`, `FooView`) are
//!    always `pub` and re-exported to the root.
//! 2. Any other type a component's public API forces a caller to name —
//!    config/style enums, constants, and action/marker types referenced by
//!    a builder's generic bounds or a view's `message()` contract
//!    (`CloseCallback`, `TogglePress`, `PopoverOpenChanged`,
//!    `DEFAULT_DELAY_MS`, `MIN_PANEL_SIZE`, ...) — is `pub` and re-exported
//!    to the root too, regardless of whether it happens to live in that
//!    component's `view.rs` or `widget.rs`.
//! 3. Masonry `Widget` struct implementations (`ThemedButton`,
//!    `CheckboxWidget`, `MenuPanel`, ...) are crate-internal. The struct
//!    itself stays `pub struct` (so `View::Element = Pod<Foo>` type-checks
//!    — see `spinner`), but its containing `mod widget;` is `pub(crate)`
//!    only when another component genuinely imports from it, else fully
//!    private. These never reach this module or the crate root.
//! 4. `mod view;` is always private — nothing outside a component needs
//!    the raw module path, since every type callers need is re-exported
//!    through the component's own `mod.rs`.

pub(crate) mod access_wrap;
pub mod alert;
pub mod autocomplete;
pub mod badge;
pub mod breadcrumb;
pub mod button;
pub mod button_group;
pub mod card;
pub mod checkbox;
pub(crate) mod click;
pub mod clipboard;
pub mod code_view;
pub mod collapsible;
pub mod context_menu;
pub mod data_grid;
pub mod date_picker;
pub mod dialog;
pub mod dropdown_button;
pub mod form;
pub mod group_box;
pub mod icon;
pub mod input;
pub(crate) mod interaction;
pub(crate) mod item_list;
pub mod kbd;
pub mod label;
pub mod list;
pub mod meter;
pub mod notification;
pub mod popover;
pub mod radio;
pub mod resizable;
pub mod scroll_container;
pub mod separator;
pub mod sidebar;
pub mod skeleton;
pub mod slider;
pub mod spinner;
pub mod status_dot;
pub mod tabs;
pub mod toggle;
pub mod tooltip;

pub use alert::{Alert, AlertVariant, CloseCallback, alert};
pub use autocomplete::{Autocomplete, AutocompleteAction, AutocompleteView, autocomplete};
pub use badge::{Badge, DismissCallback, badge, pill};
pub use breadcrumb::{Breadcrumb, BreadcrumbSegment, breadcrumb, segment};
pub use button::{
    Button, ButtonVariant, ButtonView, ContentButton, ContentButtonView, button, content_button,
};
pub use button_group::{ButtonGroup, button_group, toggle_button_group};
pub use card::{Card, card};
pub use checkbox::{Checkbox, CheckboxPress, CheckboxView, checkbox};
pub use clipboard::{Clipboard, ClipboardView, clipboard};
pub use code_view::{
    Highlighter, ReadOnlyText, ReadOnlyTextView, RustHighlighter, TokenKind, TokenSpan,
    read_only_text,
};
pub use collapsible::{Collapsible, CollapsibleView, collapsible};
pub use context_menu::{ContextMenuAction, ContextMenuArea};
pub use context_menu::{
    ContextMenuAreaBuilder, ContextMenuAreaView, Menu, MenuItem, MenuView, Submenu,
    context_menu_area, item, menu, submenu,
};
pub use data_grid::{
    CellAlign, ClickableRow, ColumnDef, ColumnId, ColumnWidths, DataGrid, ExpansionState,
    FilterState, MIN_COLUMN_WIDTH, RowClickAction, RowComparator, RowFilter, ScrollState,
    SelectionState, SortDirection, SortState, clickable_row, colored_text_column, data_grid,
    filtered_indices, optional_text_column, sort_indices, text_column,
};
pub use date_picker::{DatePicker, DatePickerAction, DatePickerView, date_picker};
pub use dialog::{Dialog, DialogView, dialog};
pub use dropdown_button::{DropdownButton, DropdownButtonView, dropdown_button};
pub use form::{Form, FormField, FormOrientation, form, form_field};
pub use group_box::{GroupBox, NoTitle, TitleState, WithTitle, group_box};
pub use icon::{Icon, IconName, disclosure_chevron, disclosure_icon, icon};
pub use input::{
    CurrencyFormat, CurrencyInput, Input, MaskedInput, NumberInput, currency_input,
    format_currency, format_mask, input, masked_input, number_input,
};
pub use kbd::{Kbd, Modifier, kbd};
pub use label::{Label, LabelAlignment, label};
pub use list::{List, list};
pub use meter::{Meter, MeterFill, meter};
pub use notification::{
    DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, Notification, NotificationLayerView,
    NotificationPosition, NotificationView, OnClose, notification, notification_layer,
    notification_overlay, notification_stack,
};
pub use popover::{Popover, PopoverAnchor, PopoverOpenChanged, PopoverView, popover};
pub use radio::{Radio, RadioView, radio};
pub use resizable::{
    MIN_PANEL_SIZE, Resizable, ResizablePanel, ResizablePanels, ResizablePanelsView, ResizableView,
    h_resizable, h_resizable_panels, v_resizable, v_resizable_panels,
};
pub use scroll_container::{
    ScrollBarVisibility, ScrollContainer, ScrollContainerView, scroll_container,
};
pub use separator::{Orientation, Separator, SeparatorStyle, separator};
pub use sidebar::{
    SidebarItem, SidebarItemView, SidebarNav, SidebarNavItem, SidebarNavView, SidebarPanel,
    SidebarPanelView, sidebar_item, sidebar_nav, sidebar_panel,
};
pub use skeleton::{Skeleton, SkeletonAnimation, SkeletonShape, SkeletonView, skeleton};
pub use slider::{RangeSlider, RangeSliderView, Slider, SliderView, range_slider, slider};
pub use spinner::{Spinner, SpinnerView, spinner};
pub use status_dot::{StatusDot, status_dot};
pub use tabs::{TabItem, Tabs, TabsVariant, TabsView, tabs};
pub use toggle::{Toggle, TogglePress, ToggleView, toggle};
pub use tooltip::{DEFAULT_DELAY_MS, Tooltip, TooltipView, tooltip};

/// One entry per component the gallery exposes.
///
/// The gallery uses this enum for two things:
/// - to iterate components when rendering its sidebar
/// - to dispatch to the focused component's `demo::panel` in the main pane
///
/// Adding a new component is one variant + one dispatch arm in the gallery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentKind {
    Alert,
    Autocomplete,
    Badge,
    Breadcrumb,
    Button,
    ButtonGroup,
    Card,
    DropdownButton,
    Checkbox,
    Clipboard,
    Form,
    GroupBox,
    CodeView,
    Collapsible,
    ContextMenu,
    DataGrid,
    DatePicker,
    Dialog,
    Icon,
    Input,
    Kbd,
    Label,
    List,
    Meter,
    Notification,
    Popover,
    Radio,
    Resizable,
    ScrollContainer,
    Separator,
    Sidebar,
    Skeleton,
    Slider,
    Spinner,
    StatusDot,
    Tabs,
    Toggle,
    Tooltip,
}

impl ComponentKind {
    /// Human-readable name for the sidebar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Alert => "Alert",
            Self::Autocomplete => "Autocomplete",
            Self::Badge => "Badge",
            Self::Breadcrumb => "Breadcrumb",
            Self::Button => "Button",
            Self::ButtonGroup => "Button Group",
            Self::Card => "Card",
            Self::DropdownButton => "Dropdown Button",
            Self::Checkbox => "Checkbox",
            Self::Clipboard => "Clipboard",
            Self::CodeView => "Code View",
            Self::Form => "Form",
            Self::GroupBox => "Group Box",
            Self::Collapsible => "Collapsible",
            Self::ContextMenu => "Context Menu",
            Self::DataGrid => "Data Grid",
            Self::DatePicker => "Date Picker",
            Self::Dialog => "Dialog",
            Self::Icon => "Icon",
            Self::Input => "Input",
            Self::Kbd => "Kbd",
            Self::Label => "Label",
            Self::List => "List",
            Self::Meter => "Meter",
            Self::Notification => "Notification",
            Self::Popover => "Popover",
            Self::Radio => "Radio",
            Self::Resizable => "Resizable",
            Self::ScrollContainer => "Scroll Container",
            Self::Separator => "Separator",
            Self::Sidebar => "Sidebar",
            Self::Skeleton => "Skeleton",
            Self::Slider => "Slider",
            Self::Spinner => "Spinner",
            Self::StatusDot => "Status Dot",
            Self::Tabs => "Tabs",
            Self::Toggle => "Toggle",
            Self::Tooltip => "Tooltip",
        }
    }

    /// Every component in display order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Alert,
            Self::Autocomplete,
            Self::Badge,
            Self::Breadcrumb,
            Self::Button,
            Self::ButtonGroup,
            Self::Card,
            Self::Checkbox,
            Self::Clipboard,
            Self::CodeView,
            Self::Collapsible,
            Self::ContextMenu,
            Self::DataGrid,
            Self::DatePicker,
            Self::Dialog,
            Self::DropdownButton,
            Self::Form,
            Self::GroupBox,
            Self::Icon,
            Self::Input,
            Self::Kbd,
            Self::Label,
            Self::List,
            Self::Meter,
            Self::Notification,
            Self::Popover,
            Self::Radio,
            Self::Resizable,
            Self::ScrollContainer,
            Self::Separator,
            Self::Sidebar,
            Self::Skeleton,
            Self::Slider,
            Self::Spinner,
            Self::StatusDot,
            Self::Tabs,
            Self::Toggle,
            Self::Tooltip,
        ]
    }
}
