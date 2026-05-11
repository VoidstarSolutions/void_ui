//! Xilem `View` wrapper for the [`super::widget::FlexWrap`] masonry
//! widget.
//!
//! Mirrors the xilem `grid` view structurally: a [`FlexWrapElement`]
//! that holds a single erased child pod, a [`FlexWrapSplice`] that
//! diffs the child list into the live widget via its
//! [`CollectionWidget`] impl, and a [`FlexWrapSequence`] marker trait
//! that lets tuples of mixed-type child views compose naturally — just
//! like `flex_row((label, button, chip))`.

use std::marker::PhantomData;

use masonry::core::{CollectionWidget, FromDynWidget, Widget, WidgetMut};
use masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};

use xilem::core::{
    AppendVec, ElementSplice, MessageCtx, MessageResult, Mut, SuperElement, View, ViewElement,
    ViewMarker, ViewSequence,
};
use xilem::{Pod, ViewCtx};

use super::widget;

/// Creates a wrapping flex container view from a sequence of child
/// views.
///
/// Children pack left-to-right and wrap to a new row when they would
/// overflow the available width. See the layout module docs for the
/// algorithm; see the builder methods on [`FlexWrap`] for the
/// configuration surface.
pub fn flex_wrap<State, Action, Seq>(sequence: Seq) -> FlexWrap<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: FlexWrapSequence<State, Action>,
{
    FlexWrap {
        sequence,
        row_gap: 0.0,
        col_gap: 0.0,
        main_axis_alignment: MainAxisAlignment::Start,
        cross_axis_alignment: CrossAxisAlignment::Start,
        phantom: PhantomData,
    }
}

/// Wrapping flex container view. Construct via [`flex_wrap`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct FlexWrap<Seq, State, Action = ()> {
    sequence: Seq,
    row_gap: f64,
    col_gap: f64,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<Seq, State, Action> FlexWrap<Seq, State, Action> {
    /// Sets both `row_gap` and `col_gap` to the same value.
    pub fn gap(mut self, gap: f64) -> Self {
        self.row_gap = gap;
        self.col_gap = gap;
        self
    }

    /// Spacing between rows.
    pub fn row_gap(mut self, gap: f64) -> Self {
        self.row_gap = gap;
        self
    }

    /// Spacing between children within a row.
    pub fn col_gap(mut self, gap: f64) -> Self {
        self.col_gap = gap;
        self
    }

    /// How children are distributed within a row.
    pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    /// How children align vertically inside their row.
    pub fn cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }
}

mod hidden {
    use xilem::core::AppendVec;

    use super::FlexWrapElement;

    #[doc(hidden)]
    #[expect(
        unnameable_types,
        reason = "implementation detail; public solely because of trait visibility rules"
    )]
    pub struct FlexWrapState<SeqState> {
        pub(crate) seq_state: SeqState,
        pub(crate) scratch: AppendVec<FlexWrapElement>,
    }
}

use hidden::FlexWrapState;

impl<Seq, State, Action> ViewMarker for FlexWrap<Seq, State, Action> {}

impl<State, Action, Seq> View<State, Action, ViewCtx> for FlexWrap<Seq, State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: FlexWrapSequence<State, Action>,
{
    type Element = Pod<widget::FlexWrap>;
    type ViewState = FlexWrapState<Seq::SeqState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut elements = AppendVec::default();
        let mut w = widget::FlexWrap::new()
            .with_row_gap(self.row_gap)
            .with_col_gap(self.col_gap)
            .with_main_axis_alignment(self.main_axis_alignment)
            .with_cross_axis_alignment(self.cross_axis_alignment);
        let seq_state = self.sequence.seq_build(ctx, &mut elements, app_state);
        for element in elements.drain() {
            w = w.with(element.child.new_widget);
        }
        let pod = ctx.create_pod(w);
        (
            pod,
            FlexWrapState {
                seq_state,
                scratch: elements,
            },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        FlexWrapState { seq_state, scratch }: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        #[expect(
            clippy::float_cmp,
            reason = "skip the layout request only when the value is bit-identical"
        )]
        {
            if self.row_gap != prev.row_gap {
                widget::FlexWrap::set_row_gap(&mut element, self.row_gap);
            }
            if self.col_gap != prev.col_gap {
                widget::FlexWrap::set_col_gap(&mut element, self.col_gap);
            }
        }
        if self.main_axis_alignment != prev.main_axis_alignment {
            widget::FlexWrap::set_main_axis_alignment(&mut element, self.main_axis_alignment);
        }
        if self.cross_axis_alignment != prev.cross_axis_alignment {
            widget::FlexWrap::set_cross_axis_alignment(&mut element, self.cross_axis_alignment);
        }

        let mut splice = FlexWrapSplice::new(element, scratch);
        self.sequence
            .seq_rebuild(&prev.sequence, seq_state, ctx, &mut splice, app_state);
        debug_assert!(scratch.is_empty());
    }

    fn teardown(
        &self,
        FlexWrapState { seq_state, scratch }: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        let mut splice = FlexWrapSplice::new(element, scratch);
        self.sequence.seq_teardown(seq_state, ctx, &mut splice);
        debug_assert!(scratch.is_empty());
    }

    fn message(
        &self,
        FlexWrapState { seq_state, scratch }: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut splice = FlexWrapSplice::new(element, scratch);
        let result = self
            .sequence
            .seq_message(seq_state, message, &mut splice, app_state);
        debug_assert!(scratch.is_empty());
        result
    }
}

// === MARK: ELEMENT =====================================================

/// A child slot in a [`FlexWrap`] view.
///
/// `FlexWrap` carries no per-child parameters (unlike `Grid`'s
/// position+span), so this is just a thin wrapper around an erased pod.
pub struct FlexWrapElement {
    child: Pod<dyn Widget>,
}

/// Mutable reference form of [`FlexWrapElement`], used internally by
/// xilem's `Mut` trait machinery.
pub struct FlexWrapElementMut<'w> {
    parent: WidgetMut<'w, widget::FlexWrap>,
    idx: usize,
}

impl ViewElement for FlexWrapElement {
    type Mut<'w> = FlexWrapElementMut<'w>;
}

impl SuperElement<Self, ViewCtx> for FlexWrapElement {
    fn upcast(_ctx: &mut ViewCtx, child: Self) -> Self {
        child
    }

    fn with_downcast_val<R>(
        mut this: Mut<'_, Self>,
        f: impl FnOnce(Mut<'_, Self>) -> R,
    ) -> (Self::Mut<'_>, R) {
        let r = {
            let parent = this.parent.reborrow_mut();
            let reborrow = FlexWrapElementMut {
                idx: this.idx,
                parent,
            };
            f(reborrow)
        };
        (this, r)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> SuperElement<Pod<W>, ViewCtx> for FlexWrapElement {
    fn upcast(_: &mut ViewCtx, child: Pod<W>) -> Self {
        Self {
            child: child.erased(),
        }
    }

    fn with_downcast_val<R>(
        mut this: Mut<'_, Self>,
        f: impl FnOnce(Mut<'_, Pod<W>>) -> R,
    ) -> (Mut<'_, Self>, R) {
        let ret = {
            let mut child = widget::FlexWrap::get_mut(&mut this.parent, this.idx);
            let downcast = child.downcast();
            f(downcast)
        };
        (this, ret)
    }
}

// === MARK: SPLICE ======================================================

/// `ElementSplice` for a live [`widget::FlexWrap`]. Walks an
/// [`AppendVec`] of scratch elements + an index into the children,
/// translating to `CollectionWidget` calls on the masonry widget.
struct FlexWrapSplice<'w, 's> {
    idx: usize,
    element: WidgetMut<'w, widget::FlexWrap>,
    scratch: &'s mut AppendVec<FlexWrapElement>,
}

impl<'w, 's> FlexWrapSplice<'w, 's> {
    fn new(
        element: WidgetMut<'w, widget::FlexWrap>,
        scratch: &'s mut AppendVec<FlexWrapElement>,
    ) -> Self {
        Self {
            idx: 0,
            element,
            scratch,
        }
    }
}

impl ElementSplice<FlexWrapElement> for FlexWrapSplice<'_, '_> {
    fn with_scratch<R>(&mut self, f: impl FnOnce(&mut AppendVec<FlexWrapElement>) -> R) -> R {
        let ret = f(self.scratch);
        for element in self.scratch.drain() {
            widget::FlexWrap::insert(&mut self.element, self.idx, element.child.new_widget, ());
            self.idx += 1;
        }
        ret
    }

    fn insert(&mut self, element: FlexWrapElement) {
        widget::FlexWrap::insert(&mut self.element, self.idx, element.child.new_widget, ());
        self.idx += 1;
    }

    fn mutate<R>(&mut self, f: impl FnOnce(Mut<'_, FlexWrapElement>) -> R) -> R {
        let child = FlexWrapElementMut {
            parent: self.element.reborrow_mut(),
            idx: self.idx,
        };
        let ret = f(child);
        self.idx += 1;
        ret
    }

    fn skip(&mut self, n: usize) {
        self.idx += n;
    }

    fn index(&self) -> usize {
        self.idx
    }

    fn delete<R>(&mut self, f: impl FnOnce(Mut<'_, FlexWrapElement>) -> R) -> R {
        let ret = {
            let child = FlexWrapElementMut {
                parent: self.element.reborrow_mut(),
                idx: self.idx,
            };
            f(child)
        };
        widget::FlexWrap::remove(&mut self.element, self.idx);
        ret
    }
}

// === MARK: SEQUENCE TRAIT =============================================

/// Marker trait for any [`ViewSequence`] that produces
/// [`FlexWrapElement`]s. Blanket-implemented for any matching
/// `ViewSequence`, so tuples of mixed widget views Just Work — exactly
/// like xilem's stock `FlexSequence` / `GridSequence`.
pub trait FlexWrapSequence<State: 'static, Action = ()>:
    ViewSequence<State, Action, ViewCtx, FlexWrapElement>
{
}

impl<Seq, State, Action> FlexWrapSequence<State, Action> for Seq
where
    Seq: ViewSequence<State, Action, ViewCtx, FlexWrapElement>,
    State: 'static,
{
}
