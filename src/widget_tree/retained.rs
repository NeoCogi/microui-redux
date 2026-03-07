//! Retained widget handles and erased dispatch adapters.

use std::{cell::RefCell, rc::Rc};

use rs_math3d::Dimensioni;

use crate::{
    atlas::AtlasHandle,
    container::Container,
    input::{ControlState, ResourceState, WidgetBehaviourOption, WidgetOption},
    style::Style,
    widget::{widget_id_of, FrameResults, Widget, WidgetId},
    widget_ctx::WidgetCtx,
    CustomRenderArgs,
};

/// Shared ownership handle for retained widget state.
pub type WidgetHandle<T> = Rc<RefCell<T>>;

/// Wraps widget state into a retained handle.
pub fn widget_handle<T>(value: T) -> WidgetHandle<T> {
    Rc::new(RefCell::new(value))
}

pub(crate) type TreeRun = Rc<RefCell<Box<dyn FnMut(&mut Container, &mut FrameResults) + 'static>>>;
pub(crate) type TreeCustomRender = Rc<RefCell<Box<dyn FnMut(Dimensioni, &CustomRenderArgs) + 'static>>>;

pub(crate) trait WidgetStateHandleDyn {
    fn widget_id(&self) -> WidgetId;
    fn effective_widget_opt(&self) -> WidgetOption;
    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption;
    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    fn needs_input_snapshot(&self) -> bool;
    fn handle(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
}

struct WidgetStateHandle<W: Widget + 'static> {
    handle: WidgetHandle<W>,
}

impl<W: Widget + 'static> WidgetStateHandleDyn for WidgetStateHandle<W> {
    fn widget_id(&self) -> WidgetId {
        let widget = self.handle.borrow();
        widget_id_of(&*widget)
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        let widget = self.handle.borrow();
        widget.effective_widget_opt()
    }

    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption {
        let widget = self.handle.borrow();
        widget.effective_behaviour_opt()
    }

    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let widget = self.handle.borrow();
        widget.preferred_size(style, atlas, avail)
    }

    fn needs_input_snapshot(&self) -> bool {
        let widget = self.handle.borrow();
        widget.needs_input_snapshot()
    }

    fn handle(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut widget = self.handle.borrow_mut();
        widget.handle(ctx, control)
    }
}

pub(crate) fn erased_widget_state<W: Widget + 'static>(handle: WidgetHandle<W>) -> Box<dyn WidgetStateHandleDyn> {
    Box::new(WidgetStateHandle { handle })
}
