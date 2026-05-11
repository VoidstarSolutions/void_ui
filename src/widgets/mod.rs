//! Custom masonry widgets used by `void_ui` components.
//!
//! Most `void_ui` components are built by composing existing masonry
//! widgets via xilem views. A custom widget shows up here only when the
//! visual state machine has to live at the masonry layer — i.e. when
//! the framework's automatic property painting can't reach the visual
//! transition we need (hover, press, focus reacting to the live theme).

pub mod themed_button;

pub use themed_button::ThemedButton;
