//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Generic application façade over the non-generic retained window manager.
//!
//! `Context` owns the backend renderer and typed application widget-event dispatcher. Every
//! retained root, input, layout, modal, style, and display-list operation is delegated to
//! [`WindowManager`]. Raw input is routed separately inside each retained UI runtime; the
//! context-owned dispatcher only drains semantic [`crate::WidgetEvent`] ports into application
//! state.

use crate::window_manager::{LayerBinding, PopupHandle, SurfaceMutationError, Window, WindowHandle, WindowManager, WindowOption};
#[cfg(test)]
use crate::window_manager::RootId;
use crate::render::{CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, FrameInfo, RenderError, Renderer, RendererBackend};
use crate::{Dimensioni, ImageSource, KeyCode, KeyMode, MouseButton, Node, Recti, Style, TextureId};

/// Short-lived access to retained UI state owned by a [`Context`].
///
/// Ordinary application code obtains this façade through [`Context::ui`]. Context-aware event
/// handlers receive the same concrete type after retained widget traversal has released every
/// widget borrow. Keeping surface mutation on this non-generic façade prevents the renderer-owning
/// [`Context`] and event dispatch from maintaining duplicate APIs.
///
/// `Ui` owns no surfaces, application state, or renderer resources. Its lifetime is tied to the
/// exclusive `Context` or dispatch borrow that created it, so it cannot escape a transaction or
/// re-enter an active retained traversal.
pub struct Ui<'a> {
    /// Exclusive access to the context-owned surface, input, and style transaction domain.
    window_manager: &'a mut WindowManager,
}

impl<'a> Ui<'a> {
    /// Lends one already-exclusive window manager to application-facing UI operations.
    pub(crate) fn new(window_manager: &'a mut WindowManager) -> Self {
        // Construction stays private so every public capability originates at a Context-controlled
        // borrow boundary, including the post-traversal event-dispatch boundary.
        Self { window_manager }
    }

    /// Creates an open retained window from one complete body and optional menu-bar definition.
    ///
    /// The returned handle is weak; the lending [`Context`] remains the sole window owner.
    pub fn create_window(&mut self, window: Window) -> WindowHandle {
        // Transfer the complete definition to the same owner used by ordinary Context creation.
        self.window_manager.create_window(window)
    }

    /// Creates an open structural child window below `parent`'s menu and chrome.
    ///
    /// The child keeps screen-space geometry and its own retained application tree, menu, chrome,
    /// visibility, focus, and pointer capture. It inherits the fixed layer of its top-level family
    /// root and participates in chronological z-order only among windows with the same direct
    /// parent. The parent's [`crate::ChildWindowClip`] policy determines whether the complete child
    /// surface is additionally clipped to the parent application body.
    ///
    /// Destroying the parent destroys the child and its descendants. Hiding the parent excludes the
    /// complete family until the parent is shown again. Modal dialogs cannot own child windows.
    ///
    /// # Errors
    ///
    /// Returns [`SurfaceMutationError::UnknownWindow`] for a stale or foreign parent and
    /// [`SurfaceMutationError::InvalidChildWindowParent`] when the authenticated parent is modal.
    pub fn create_child_window(&mut self, parent: &WindowHandle, window: Window) -> Result<WindowHandle, SurfaceMutationError> {
        // WindowManager validates the authenticated root role before consuming the unique Window;
        // failed ownership validation therefore leaves no partially mounted child surface.
        self.window_manager.create_child_window(parent, window)
    }

    /// Creates a hidden retained dialog owned by `parent`.
    ///
    /// `parent` must be a live independent or structural child window from this Context. Show the
    /// returned dialog with [`Self::set_window_visible`]; it becomes the front modal group before
    /// the layout immediately following this UI transaction.
    pub fn create_dialog(&mut self, parent: &WindowHandle, window: Window) -> Result<WindowHandle, SurfaceMutationError> {
        // WindowManager authenticates the complete capability before consuming the dialog value.
        self.window_manager.create_dialog(parent, window)
    }

    /// Creates a hidden auto-sized popup owned directly by `parent`.
    ///
    /// The popup definition and retained content tree become a concrete child in the same forest.
    /// The distinct handle prevents popup identity from entering window-only operations.
    pub fn create_popup(&mut self, parent: &WindowHandle, name: &str, content: Node) -> Result<PopupHandle, SurfaceMutationError> {
        // Authenticate the owner capability before transferring the popup content into the forest.
        self.window_manager.create_popup(parent, name, content)
    }

    /// Replaces a retained window or dialog title before the next layout commit.
    pub fn set_window_name(&mut self, window: &WindowHandle, name: impl Into<String>) -> Result<(), SurfaceMutationError> {
        // Convert the caller-facing string once at the concrete manager boundary that owns it.
        self.window_manager.set_window_name(window, name.into())
    }

    /// Replaces a retained window or dialog rectangle before the next layout commit.
    pub fn set_window_rect(&mut self, window: &WindowHandle, rect: Recti) -> Result<(), SurfaceMutationError> {
        // The manager validates context ownership before changing authoritative chrome geometry.
        self.window_manager.set_window_rect(window, rect)
    }

    /// Replaces a retained window or dialog size without changing its origin.
    pub fn set_window_size(&mut self, window: &WindowHandle, size: Dimensioni) -> Result<(), SurfaceMutationError> {
        // Size remains authoritative manager state rather than an application-side deferred value.
        self.window_manager.set_window_size(window, size)
    }

    /// Replaces the chrome options for a retained window or dialog.
    pub fn set_window_options(&mut self, window: &WindowHandle, options: WindowOption) -> Result<(), SurfaceMutationError> {
        // Apply option-dependent capture cleanup in the concrete WindowManager implementation.
        self.window_manager.set_window_options(window, options)
    }

    /// Replaces the presentation options for one window-owned popup definition.
    ///
    /// Popup identity remains typed so this operation cannot accidentally mutate window chrome.
    pub fn set_popup_options(&mut self, popup: &PopupHandle, options: WindowOption) -> Result<(), SurfaceMutationError> {
        // Apply the option change to the retained popup definition before the next layout commit.
        self.window_manager.set_popup_options(popup, options)
    }

    /// Assigns an independent window to one of the sixteen fixed application layers.
    ///
    /// Structural children inherit their family root's layer and return
    /// [`SurfaceMutationError::ManagedLayer`] if passed here.
    pub fn set_window_layer(&mut self, window: &WindowHandle, layer: u8) -> Result<(), SurfaceMutationError> {
        // Layer validation and modal restrictions remain one WindowManager transaction at the
        // event-safe mutation boundary; popups derive the layer from their owning window.
        self.window_manager.set_window_layer(window, layer)
    }

    /// Returns a registered window's effective fixed or modal layer policy.
    ///
    /// A structural child reports its top-level family root's current fixed layer.
    pub fn window_layer(&self, window: &WindowHandle) -> Result<LayerBinding, SurfaceMutationError> {
        // Authenticate the handle rather than exposing manager storage or a forged default band.
        self.window_manager.window_layer(window)
    }

    /// Shows or hides a retained window or dialog while preserving its application tree.
    ///
    /// Hiding a window removes its complete structural family from effective visibility. Child
    /// windows retain their local visibility intent and recover when their ancestors are shown;
    /// modal descendants are explicitly closed and do not recover implicitly.
    ///
    /// Popup definitions have a distinct typed identity and cannot be passed to this generic window
    /// operation. Use [`Self::show_popup`], [`Self::show_popup_at`], or [`Self::hide_popup`] instead.
    pub fn set_window_visible(&mut self, window: &WindowHandle, visible: bool) -> Result<(), SurfaceMutationError> {
        // Mutate the window synchronously at the safe dispatch boundary so the following layout sees
        // the requested visibility without an application-owned frame flag.
        self.window_manager.set_window_visible(window, visible)
    }

    /// Shows a popup at the current pointer position under its stable owner.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), SurfaceMutationError> {
        // Placement and active-path replacement remain atomic inside the manager.
        self.window_manager.show_popup(popup)
    }

    /// Hides an active popup and every active descendant while retaining their definitions.
    ///
    /// Closing any active suffix records one [`crate::PopupEvent::Dismissed`] event per removed
    /// application popup.
    pub fn hide_popup(&mut self, popup: &PopupHandle) -> Result<(), SurfaceMutationError> {
        // Delegate path truncation to the manager so visibility has one authoritative source.
        self.window_manager.hide_popup(popup)
    }

    /// Shows a popup at an exact screen-space anchor before the following layout commit.
    ///
    /// This atomic form is intended for composed controls such as combos. It applies popup-path
    /// replacement and replaces the popup rectangle, so no pointer-relative intermediate placement
    /// can be observed. The typed handle prevents passing a window or dialog as the popup target.
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), SurfaceMutationError> {
        // Delegate the complete transaction while retaining compile-time popup identity across the
        // event façade boundary.
        self.window_manager.show_popup_at(popup, anchor)
    }

    /// Raises a registered window or dialog inside its effective layer.
    ///
    /// This operation never moves an ordinary window across another numeric application layer. An
    /// independent window raises its complete family among fixed-layer peers; a child raises only
    /// among siblings with the same direct parent. A visible dialog moves its modal group in front
    /// and closes popups from the previous modal group. A stale or foreign handle returns a checked
    /// mutation error.
    pub fn bring_window_to_front(&mut self, window: &WindowHandle) -> Result<(), SurfaceMutationError> {
        // Let WindowManager raise the authenticated node inside its structural stacking band.
        self.window_manager.bring_window_to_front(window)
    }

    /// Permanently unregisters a window or dialog and its complete structural subtree.
    ///
    /// Every child window, dialog, and popup below the target is dropped, making its stable handles
    /// stale and expiring its weak widget and event endpoints.
    pub fn destroy_window(&mut self, window: &WindowHandle) -> Result<(), SurfaceMutationError> {
        // Destruction also expires every weak application widget and event handle in the subtree.
        self.window_manager.destroy_window(window)
    }

    /// Borrows the concrete state of one mounted menu item.
    pub fn menu_item(&self, handle: &crate::MenuItemHandle) -> Result<&crate::MenuItemParameters, crate::MenuItemAccessError> {
        // The handle's private stable ID selects manager-owned state; its event endpoint remains an
        // independent subscription capability.
        self.window_manager.menu_item(handle)
    }

    /// Mutably borrows the concrete state of one mounted menu item.
    pub fn menu_item_mut(&mut self, handle: &crate::MenuItemHandle) -> Result<&mut crate::MenuItemParameters, crate::MenuItemAccessError> {
        // WindowManager invalidates layout conservatively before lending any public field mutably.
        self.window_manager.menu_item_mut(handle)
    }

    /// Returns the active popup names at the event-dispatch boundary for internal tests.
    #[cfg(test)]
    pub(crate) fn debug_active_popup_names(&self) -> Vec<String> {
        // This observation proves ordering only: retained updates and manager menu policy have both
        // completed before an application handler receives the item's typed submission.
        self.window_manager.debug_active_popup_names()
    }

    /// Returns the resolved UI style currently used by the owning context.
    pub fn style(&self) -> &Style {
        // The manager owns the resolved style used by layout, input presentation, and paint.
        self.window_manager.style()
    }
}

/// Primary entry point used to drive the UI over a rendering backend.
///
/// `Context` is the only public ordered input-queue boundary. Input forwarding calls append raw
/// transitions without coalescing; [`Context::update_ui`] drains them in call order. The exception
/// to ordinary root hit routing is the popup boundary: an outside pointer press dismisses the
/// active popup before the event may continue to the root underneath.
///
/// Across ordinary roots, pointer hover and new presses follow topmost hit geometry. Within a child
/// family, parent menu/chrome precedes children, children precede the parent body, and later siblings
/// precede earlier ones. A press raises its target only within the corresponding top-level or sibling
/// scope and records the ordinary `active_root` independently. Drag remains confined by pointer
/// capture or the front eligible visual root, while wheel input follows the topmost eligible root
/// under the pointer. Keyboard and text return to the captured or active root. Captured pointer
/// release still returns to its widget so local drag state is cleaned up.
///
/// The frontmost visible dialog is modal. It occupies the dedicated band above all application
/// layers, and the dialog with its active popup path forms the only eligible input group. Pointer
/// input outside that group is consumed at the cross-window boundary; other windows remain visible and continue to
/// participate in layout and paint.
///
/// `Context`, its retained state, and its registered custom-render callbacks stay on the thread
/// that owns the context. The rendering contracts intentionally do not require `Send` or `Sync`;
/// applications should deliver any cross-thread results before starting a [`ContextFrame`].
///
/// A live [`ContextFrame`] exclusively owns the Context borrow, preventing input/resource
/// mutation or another logical frame until it is rendered or cancelled:
///
/// ```compile_fail
/// use microui_redux::Context;
/// use microui_redux::render::{FrameInfo, RendererBackend};
///
/// fn mutate_during_frame<B: RendererBackend>(context: &mut Context<B>, info: FrameInfo) {
///     let frame = context.frame(info);
///     context.mousemove(10, 20);
///     drop(frame);
/// }
/// ```
pub struct Context<B: RendererBackend, State: 'static = ()> {
    /// High-level renderer that replays root display lists.
    renderer: Renderer<B>,
    /// Backend- and application-state-independent retained window manager.
    pub(crate) window_manager: WindowManager,
    /// Sole semantic widget-event dispatcher for this context and its retained widget forest.
    widget_event_dispatcher: crate::event::WidgetEventDispatcher<State>,
    /// Drawable size used by retained behavior tests that drive complete frames tersely.
    #[cfg(test)]
    test_dimensions: Dimensioni,
}

/// Exclusively owned logical UI frame.
///
/// This value borrows `Context` to serialize paint/submission, but it does not lock independent
/// [`crate::TypedWidgetHandle`] access. Mutating layout-affecting state after the last update commit
/// makes that commit semantically stale; drop the unsubmitted frame and call
/// [`Context::update_ui`] or [`Context::update_ui_state`] again before painting. No separate
/// Context token exists.
///
/// Submission consumes the frame, making a second submission unrepresentable:
///
/// ```compile_fail
/// use microui_redux::Context;
/// use microui_redux::render::{FrameInfo, RendererBackend};
///
/// fn submit_twice<B: RendererBackend>(context: &mut Context<B>, info: FrameInfo) {
///     let frame = context.frame(info);
///     frame.render_ui().unwrap();
///     frame.render_ui().unwrap();
/// }
/// ```
#[must_use = "call render_ui() to submit this UI frame; dropping it cancels"]
pub struct ContextFrame<'a, B: RendererBackend, State: 'static = ()> {
    context: &'a mut Context<B, State>,
    info: FrameInfo,
    completed: bool,
}

// Construction and test fixtures.

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Creates a new UI context with unique ownership of the provided backend.
    ///
    /// The default style binds conventional semantic font and icon names from the backend atlas.
    pub fn new(backend: B) -> Self {
        // The backend supplies the atlas; the default style then binds semantic assets from it.
        let renderer = Renderer::new(backend);
        let style = Style::default().with_named_assets(&renderer.atlas());
        Self {
            renderer,
            window_manager: WindowManager::new(style),
            widget_event_dispatcher: crate::event::WidgetEventDispatcher::new(),
            #[cfg(test)]
            test_dimensions: Dimensioni::new(1, 1),
        }
    }

    /// Borrows the context-owned retained UI domain for root and popup operations.
    ///
    /// The returned concrete façade is the same capability supplied to context-aware event
    /// handlers. Its borrow ends independently of renderer and event-dispatcher APIs, allowing
    /// application code to keep those concerns separate without duplicating manager methods on
    /// `Context` itself.
    pub fn ui(&mut self) -> Ui<'_> {
        // Restrict the borrow to WindowManager so the façade remains backend- and State-independent.
        Ui::new(&mut self.window_manager)
    }

    /// Creates a Context whose test-only frame helper uses `dimensions`.
    #[cfg(test)]
    pub(crate) fn new_test_state(backend: B, dimensions: Dimensioni) -> Self {
        let mut context = Self::new(backend);
        context.test_dimensions = dimensions;
        context
    }
}

impl<B: RendererBackend> Context<B> {
    #[cfg(test)]
    pub(crate) fn new_test(backend: B, dimensions: Dimensioni) -> Self {
        Self::new_test_state(backend, dimensions)
    }

    /// Updates and renders once for retained behavior tests.
    #[cfg(test)]
    pub(crate) fn update_and_render_ui(&mut self) {
        let info = FrameInfo::try_new(self.test_dimensions, crate::color(0, 0, 0, 0)).expect("test Context dimensions must be positive");
        self.update_ui(self.test_dimensions);
        self.frame(info).render_ui().expect("test backend frame should render");
    }
}

// Retained update and application-event dispatch.

impl<B: RendererBackend> Context<B> {
    /// Drains input and commits layout for a polling-only context.
    #[track_caller]
    pub fn update_ui(&mut self, dimensions: Dimensioni) {
        assert!(dimensions.width > 0 && dimensions.height > 0, "update_ui dimensions must be positive");
        let atlas = self.renderer.atlas();
        self.window_manager.update(dimensions, &atlas);
    }
}

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Drains ordered input, dispatches typed retained UI events, and commits layout for `dimensions`.
    ///
    /// One synchronization layout always runs first. Each queued input event then causes exactly
    /// one route followed by one full eligible-tree update and another layout commit. Geometry
    /// produced for one event is therefore authoritative when routing the next. With an empty
    /// queue, the initial layout is the complete synchronization commit. This method performs no
    /// timer synthesis, painting, or backend submission.
    ///
    /// Events retain FIFO order within each retained source port. When multiple ports are ready at one
    /// dispatch boundary, they are drained in subscription order. Dispatch repeats until all
    /// subscribed ports are empty, including events emitted by application-state methods.
    /// Context-aware subscribers receive [`Ui`] only at these boundaries; their root and
    /// root mutations complete before the layout commit for the current raw input event.
    ///
    /// Context-owned input, style, and root mutations invalidate a prior commit automatically.
    /// Mutations made through weak typed widget handles cannot notify Context; callers
    /// must invoke this method after those mutations, including when the input queue is empty.
    /// Callers must finish every typed-access closure first. A closure retains its widget-cell borrow;
    /// if this traversal reaches that cell and requests an incompatible borrow, built-in runtimes
    /// panic with the retained-state invariant diagnostic rather than skipping work or committing
    /// stale state.
    #[track_caller]
    pub fn update_ui_state(&mut self, dimensions: Dimensioni, state: &mut State) {
        assert!(dimensions.width > 0 && dimensions.height > 0, "update_ui_state dimensions must be positive");
        let atlas = self.renderer.atlas();
        // Split the two Context-owned transaction participants before entering WindowManager. The
        // dispatch closure can then lend the manager back through Ui without aliasing the
        // independently borrowed application dispatcher.
        let window_manager = &mut self.window_manager;
        let widget_event_dispatcher = &mut self.widget_event_dispatcher;
        window_manager.update_with(dimensions, &atlas, state, |window_manager, state| {
            // This closure runs only after complete retained traversals release widget borrows.
            let mut ui = Ui::new(window_manager);
            widget_event_dispatcher.dispatch_with_context(state, &mut ui)
        });
    }

    /// Subscribes the context's application state to one typed retained UI event.
    ///
    /// A retained event port accepts one subscription and returns
    /// [`crate::SubscribeError::AlreadySubscribed`] for another.
    pub fn subscribe<E: crate::WidgetEvent>(&mut self, port: crate::WidgetEventPortHandle<E>, method: fn(&mut State, &E)) -> Result<(), crate::SubscribeError> {
        self.widget_event_dispatcher.subscribe(port, method)
    }

    /// Subscribes the context's application state with one bound application value.
    ///
    /// A retained event port accepts one subscription and returns
    /// [`crate::SubscribeError::AlreadySubscribed`] for another.
    pub fn subscribe_with<E: crate::WidgetEvent, BoundContext: 'static>(
        &mut self,
        port: crate::WidgetEventPortHandle<E>,
        context: BoundContext,
        method: fn(&mut State, &BoundContext, &E),
    ) -> Result<(), crate::SubscribeError> {
        self.widget_event_dispatcher.subscribe_with(port, context, method)
    }

    /// Subscribes a state method that also needs safe context-owned UI mutation access.
    ///
    /// The supplied [`Ui`] exists only for one dispatch call after retained widget borrows
    /// have ended. Root mutations performed through it are committed by the layout immediately
    /// following that dispatch boundary. Use [`Context::subscribe`] when the handler only mutates
    /// application or widget state.
    pub fn subscribe_context<E: crate::WidgetEvent>(
        &mut self,
        port: crate::WidgetEventPortHandle<E>,
        method: for<'a> fn(&mut State, &mut Ui<'a>, &E),
    ) -> Result<(), crate::SubscribeError> {
        // Store the typed function pointer in the same context-owned dispatcher as state-only
        // subscriptions; only its invocation adapter differs.
        self.widget_event_dispatcher.subscribe_context(port, method)
    }

    /// Subscribes a context-aware state method with one immutable bound application value.
    ///
    /// The bound value precedes [`Ui`] and the event payload in the method signature,
    /// matching the established [`Context::subscribe_with`] argument order.
    pub fn subscribe_context_with<E: crate::WidgetEvent, BoundContext: 'static>(
        &mut self,
        port: crate::WidgetEventPortHandle<E>,
        context: BoundContext,
        method: for<'a> fn(&mut State, &BoundContext, &mut Ui<'a>, &E),
    ) -> Result<(), crate::SubscribeError> {
        // The dispatcher owns the bound value and preserves ordinary subscription ordering.
        self.widget_event_dispatcher.subscribe_context_with(port, context, method)
    }
}

// Ordered input forwarding.

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Queues one mouse-pointer position transition without coalescing.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.window_manager.mousemove(x, y);
    }

    /// Queues one mouse-button press at the supplied pointer position.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.window_manager.mousedown(x, y, btn);
    }

    /// Queues one mouse-button release at the supplied pointer position.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.window_manager.mouseup(x, y, btn);
    }

    /// Queues one scroll-wheel or trackpad delta without accumulating adjacent calls.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.window_manager.scroll(x, y);
    }

    /// Queues one modifier/control-key press.
    pub fn keydown(&mut self, key: KeyMode) {
        self.window_manager.keydown(key);
    }

    /// Queues one modifier/control-key release.
    pub fn keyup(&mut self, key: KeyMode) {
        self.window_manager.keyup(key);
    }

    /// Queues one navigation-key press.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.window_manager.keydown_code(code);
    }

    /// Queues one navigation-key release.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.window_manager.keyup_code(code);
    }

    /// Queues one UTF-8 text transition, including an empty string.
    ///
    /// Widgets retain the complete string. Rendering remains limited to glyphs in the selected
    /// atlas font and substitutes underscore metrics for missing characters.
    pub fn text(&mut self, text: &str) {
        self.window_manager.text(text);
    }
}

// Style, backend callbacks, and renderer resources.

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Registers one backend-specific callback for retained custom-render nodes.
    ///
    /// The callback is observational with respect to retained application state, topology,
    /// interaction, and layout. It may mutate callback-private rendering caches, but using a
    /// captured [`TypedWidgetHandle`](crate::TypedWidgetHandle) to mutate retained UI during frame
    /// execution is a contract violation rather than a deferred-next-frame update.
    ///
    /// A callback written for another backend frame type cannot be registered:
    ///
    /// ```compile_fail
    /// use microui_redux::{Context, CustomRenderArgs};
    /// use microui_redux::render::RendererBackend;
    ///
    /// fn register_for_wrong_backend<A, B, F>(context: &mut Context<A>, callback: F)
    /// where
    ///     A: RendererBackend,
    ///     B: RendererBackend,
    ///     F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
    /// {
    ///     context.register_custom_renderer(callback).unwrap();
    /// }
    /// ```
    ///
    /// The active frame borrow cannot escape the callback invocation:
    ///
    /// ```compile_fail
    /// use microui_redux::{Context, CustomRenderArgs};
    /// use microui_redux::render::RendererBackend;
    ///
    /// fn retain_frame<B: RendererBackend>(context: &mut Context<B>) {
    ///     let mut retained = None;
    ///     context.register_custom_renderer(
    ///         move |frame: &mut B::Frame<'_>, _args: CustomRenderArgs| {
    ///             retained = Some(frame);
    ///         },
    ///     ).unwrap();
    /// }
    /// ```
    pub fn register_custom_renderer<F>(&mut self, callback: F) -> Result<CustomRenderHandle<B>, CustomRenderRegistryError>
    where
        F: for<'frame> FnMut(&mut B::Frame<'frame>, CustomRenderArgs) + 'static,
    {
        self.renderer.register_custom_renderer(callback)
    }

    /// Removes a previously registered custom-render callback.
    pub fn unregister_custom_renderer(&mut self, handle: CustomRenderHandle<B>) -> Result<(), CustomRenderRegistryError> {
        self.renderer.unregister_custom_renderer(handle)
    }

    /// Replaces the current UI style.
    ///
    /// Unset/default font and icon fields are rebound automatically from the current atlas when it
    /// exposes their conventional semantic names. Use [`Style::with_named_assets`] or
    /// [`Style::bind_named_assets`] when you want to force all semantic roles to those atlas
    /// bindings explicitly.
    pub fn set_style(&mut self, style: &Style) {
        let mut resolved = *style;
        resolved.bind_default_named_fonts(&self.renderer.atlas());
        resolved.bind_default_named_icons(&self.renderer.atlas());
        self.window_manager.set_style(resolved);
    }

    /// Returns the resolved UI style currently used by this context.
    pub fn style(&self) -> &Style {
        self.window_manager.style()
    }

    /// Returns the high-level renderer used for frame execution and resource management.
    ///
    /// Application code should prefer the higher-level context image APIs and retained widget
    /// rendering. Backend integrations can use this accessor for atlas metadata.
    pub fn renderer(&self) -> &Renderer<B> {
        &self.renderer
    }

    /// Attempts to upload an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Dimensions and byte length are validated before an id is allocated. Backend upload errors
    /// are returned without recording texture state in the renderer.
    pub fn try_load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, String> {
        self.renderer.try_load_texture_rgba(width, height, pixels)
    }

    /// Uploads an RGBA image to the renderer and returns its [`TextureId`].
    ///
    /// Panics if the RGBA dimensions/byte length are invalid or the backend rejects the upload.
    /// Prefer [`Context::try_load_image_rgba`] when callers can handle upload failure.
    #[track_caller]
    pub fn load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId {
        self.try_load_image_rgba(width, height, pixels).expect("failed to upload RGBA image")
    }

    /// Deletes a previously uploaded texture.
    ///
    /// Deleting an unknown or already-freed handle triggers a debug assertion and is an idempotent
    /// no-op in release builds.
    pub fn free_image(&mut self, id: TextureId) {
        self.renderer.free_texture(id);
    }

    /// Uploads texture data described by `source`. PNG decoding is only available when the
    /// `png_source` (or `builder`) feature is enabled.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, String> {
        match source {
            ImageSource::Raw { width, height, pixels } => self.try_load_image_rgba(width, height, pixels),
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, colors) = crate::image::load_image_bytes(ImageSource::Png { bytes }).map_err(|error| error.to_string())?;
                let width = i32::try_from(width).map_err(|_| String::from("PNG width exceeds supported range"))?;
                let height = i32::try_from(height).map_err(|_| String::from("PNG height exceeds supported range"))?;
                let rgba: Vec<u8> = colors.into_iter().flat_map(|color| [color.x, color.y, color.z, color.w]).collect();
                self.try_load_image_rgba(width, height, rgba.as_slice())
            }
        }
    }
}

// Test-only retained-state inspection.

#[cfg(test)]
impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Returns visible window and popup names in base-surface paint order for internal tests.
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        // Names expose ordering policy without leaking private stacking records.
        self.window_manager.debug_rendered_root_names()
    }

    /// Returns the manager-owned root name for internal behavioral tests.
    pub(crate) fn debug_root_name(&self, root: RootId) -> Option<String> {
        // Forward through the test-only façade without exposing production chrome access.
        self.window_manager.debug_root_name(root)
    }

    /// Returns the authoritative root rectangle for internal behavioral tests.
    pub(crate) fn debug_root_rect(&self, root: RootId) -> Option<Recti> {
        // Copy geometry out so tests cannot retain references into manager storage.
        self.window_manager.debug_root_rect(root)
    }

    /// Returns the committed effective surface clip for internal hierarchy tests.
    pub(crate) fn debug_root_clip(&self, root: RootId) -> Option<Recti> {
        // Copy the private snapshot used jointly by retained paint and cross-window hit testing.
        self.window_manager.debug_root_clip(root)
    }

    /// Returns whether a registered root is visible for internal behavioral tests.
    pub(crate) fn debug_root_visible(&self, root: RootId) -> Option<bool> {
        // Preserve `None` for an unknown or destroyed root.
        self.window_manager.debug_root_visible(root)
    }

    /// Returns a window-owned popup's authoritative rectangle for internal tests.
    pub(crate) fn debug_popup_rect(&self, popup: &PopupHandle) -> Option<Recti> {
        // Keep popup inspection typed just like the production mutation façade.
        self.window_manager.debug_popup_rect(popup)
    }

    /// Returns whether a popup belongs to the manager's active path for internal tests.
    pub(crate) fn debug_popup_visible(&self, popup: &PopupHandle) -> Option<bool> {
        // Popup definitions store no visibility mirror, so the manager derives this from its path.
        self.window_manager.debug_popup_visible(popup)
    }

    /// Returns the laid-out content size of a window-owned popup for internal tests.
    pub(crate) fn debug_popup_content_size(&self, popup: &PopupHandle) -> Option<Dimensioni> {
        // Keep layout inspection behind the typed handle so popup storage remains private.
        self.window_manager.debug_popup_content_size(popup)
    }

    /// Returns one retained node rectangle inside a window-owned popup for internal tests.
    pub(crate) fn debug_popup_node_rect(&self, popup: &PopupHandle, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        // The manager validates both the popup handle and retained node before returning geometry.
        self.window_manager.debug_popup_node_rect(popup, node)
    }

    /// Returns the active popup branch in parent-to-child order for internal tests.
    pub(crate) fn debug_active_popup_names(&self) -> Vec<String> {
        // Names make path assertions readable without exposing private popup identifiers.
        self.window_manager.debug_active_popup_names()
    }

    /// Returns active popup rectangles in parent-to-child order for internal anchor tests.
    pub(crate) fn debug_active_popup_rects(&self) -> Vec<Recti> {
        // Geometry order matches the authoritative popup path used by layout and paint.
        self.window_manager.debug_active_popup_rects()
    }

    /// Returns relational menu trigger rectangles in their compiled declaration order for tests.
    pub(crate) fn debug_menu_anchor_rects(&self, root: RootId) -> Option<Vec<Option<Recti>>> {
        // The manager resolves private trigger identities; Context exposes only copied geometry.
        self.window_manager.debug_menu_anchor_rects(root)
    }

    /// Returns the committed full menu-bar allocation for root layout tests.
    pub(crate) fn debug_menu_bar_rect(&self, root: RootId) -> Option<Recti> {
        self.window_manager.debug_menu_bar_rect(root)
    }

    /// Returns active compact menu rows in parent-to-child popup order for tests.
    pub(crate) fn debug_active_menu_row_rects(&self) -> Vec<Vec<Recti>> {
        // Logical declaration slots replace the removed per-row RuntimeNodeId identities.
        self.window_manager.debug_active_menu_row_rects()
    }

    /// Returns whether any manager-owned chrome gesture is active for tests.
    pub(crate) fn debug_root_active(&self, root: RootId) -> Option<bool> {
        // This intentionally excludes application widget capture details.
        self.window_manager.debug_root_active(root)
    }

    /// Returns whether a title move gesture is active for internal tests.
    pub(crate) fn debug_root_moving(&self, root: RootId) -> Option<bool> {
        // Delegate exact private interaction inspection to WindowManager.
        self.window_manager.debug_root_moving(root)
    }

    /// Returns whether a resize gesture is active for internal tests.
    pub(crate) fn debug_root_resizing(&self, root: RootId) -> Option<bool> {
        // Delegate exact private interaction inspection to WindowManager.
        self.window_manager.debug_root_resizing(root)
    }

    /// Returns the currently active ordinary window for internal routing tests.
    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        // Activation is intentionally distinct from z-order and modal selection.
        self.window_manager.debug_active_root()
    }

    /// Returns the frontmost visible dialog for internal modal-policy tests.
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        // The manager derives modal activation from flat window visibility and z-order.
        self.window_manager.debug_modal_root()
    }

    /// Returns the committed application-body rectangle for internal chrome tests.
    pub(crate) fn debug_root_body(&self, root: RootId) -> Option<Recti> {
        // Resolve font-dependent title geometry with the Context renderer's atlas.
        self.window_manager.debug_root_body(root, &self.renderer.atlas())
    }

    /// Returns retained traversal counters for one window or dialog in internal tests.
    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        // Metrics stay test-only so production traversal exposes no diagnostic state.
        self.window_manager.debug_root_runtime_metrics(root)
    }

    /// Returns whether window chrome or application content owns pointer capture in tests.
    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        // Combine both capture domains exactly as cross-window routing does.
        self.window_manager.debug_root_has_pointer_capture(root)
    }

    /// Counts application-authored retained nodes inside one window for internal tests.
    pub(crate) fn debug_root_node_count(&self, root: RootId) -> Option<usize> {
        // Manager chrome is intentionally absent from this application topology count.
        self.window_manager.debug_root_node_count(root)
    }

    /// Returns one retained application node rectangle inside a window for internal tests.
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        // Resolve the node through the window-local runtime without exposing that runtime.
        self.window_manager.debug_root_node_rect(root, node)
    }

    /// Returns title, close, and resize rectangles for one window in internal tests.
    pub(crate) fn debug_root_chrome(&self, root: RootId) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        // Use the renderer atlas so test geometry matches production title measurement.
        self.window_manager.debug_root_chrome(root, &self.renderer.atlas())
    }
}

// Logical frame ownership and submission.

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Starts one paint/submission frame for state previously committed by
    /// [`Context::update_ui`] or [`Context::update_ui_state`].
    pub fn frame(&mut self, info: FrameInfo) -> ContextFrame<'_, B, State> {
        ContextFrame { context: self, info, completed: false }
    }
}

impl<B: RendererBackend, State: 'static> ContextFrame<'_, B, State> {
    /// Consumes this logical frame, paints the last committed UI once, and submits it once.
    ///
    /// Returns [`RenderError::UiUpdateRequired`] before paint or backend acquisition when no commit
    /// exists for these dimensions or when raw input is pending. This operation is paint-only: it
    /// does not route input, update semantic state, run layout, synthesize timers, or produce a
    /// generic frame-result/resource-state object. Widget paint is observational with respect to
    /// application-authored semantic state, topology, interaction, and committed layout. Widgets
    /// may update private rendering caches; custom-render callbacks may update callback-private
    /// rendering caches only. Neither kind of cache can alter the current commit or publish
    /// application-coordination events.
    pub fn render_ui(mut self) -> Result<(), RenderError> {
        let dimensions = self.info.dimensions();
        if !self.context.window_manager.can_render(dimensions) {
            self.completed = true;
            return Err(RenderError::UiUpdateRequired);
        }
        let atlas = self.context.renderer.atlas();
        self.context.window_manager.paint(self.info.dimensions(), &atlas);
        let Context { renderer, window_manager, .. } = &mut *self.context;
        let result = renderer.render(self.info, window_manager.display_list_mut());
        self.completed = true;
        result
    }
}

impl<B: RendererBackend, State: 'static> Drop for ContextFrame<'_, B, State> {
    fn drop(&mut self) {
        if !self.completed {
            self.context.window_manager.cancel_frame();
        }
    }
}
