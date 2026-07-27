//! Motion preference — whether decorative animation should play.
//!
//! `void_ui` is presentation-only and never queries the OS's
//! `prefers-reduced-motion` (or platform equivalent) itself. Host apps detect
//! that preference and map it onto [`Motion`] before constructing or
//! mutating a [`crate::Theme`], the same way they already own swapping
//! [`crate::theme::Density`] or [`crate::theme::ThemeVariant`].
//!
//! Components that read this token must treat `reduced: true` as an
//! unconditional override of any per-instance animation choice — see
//! [`crate::components::skeleton::view::Skeleton::render`] for the reference
//! implementation. Not every animated widget honors it: [`crate::spinner()`]'s
//! rotation is its only signal that work is in progress, so it is
//! deliberately exempt (WCAG 2.3.3's "unless essential" carve-out).

/// Motion tokens components consult before playing decorative animation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    /// When `true`, components that honor this token suppress decorative
    /// animation regardless of any per-instance choice.
    pub reduced: bool,
}

impl Motion {
    /// Full motion — decorative animation plays normally. The default.
    #[must_use]
    pub const fn full() -> Self {
        Self { reduced: false }
    }

    /// Reduced motion — components that honor this token suppress
    /// decorative animation.
    #[must_use]
    pub const fn reduced() -> Self {
        Self { reduced: true }
    }
}

#[cfg(test)]
mod tests {
    use super::Motion;

    /// The default (and `Motion::full()`) is full motion — `reduced` is
    /// false, so components play their decorative animation normally.
    #[test]
    fn default_and_full_are_not_reduced() {
        assert!(!Motion::default().reduced);
        assert!(!Motion::full().reduced);
    }

    /// `Motion::reduced()` sets `reduced` to true.
    #[test]
    fn reduced_constructor_sets_the_flag() {
        assert!(Motion::reduced().reduced);
    }
}
