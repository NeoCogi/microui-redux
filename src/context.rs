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

use crate::window_manager::{LayerBinding, PopupHandle, RootHandle, RootId, RootMutationError, WindowManager, WindowOption};
use crate::render::{CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, FrameInfo, RenderError, Renderer, RendererBackend};
use crate::{Dimensioni, ImageSource, KeyCode, KeyMode, MouseButton, Node, Recti, Style, TextureId};

/// Context-owned UI mutation capability available to an application event handler.
///
/// [`Context::update_ui_state`] creates this short-lived façade only after the complete retained
/// widget update has released every widget borrow. Mutations therefore use the same authoritative
/// window-manager operations as [`Context`] without allowing handler code to re-enter an active
/// widget traversal. The capability owns no roots or renderer resources and cannot outlive the
/// dispatch call that lent it.
///
/// State-only handlers registered with [`Context::subscribe`] remain the simpler default. Use
/// [`Context::subscribe_context`] when a handler must create, show, hide, move, resize, raise, or
/// destroy a retained root in direct response to a typed retained UI event.
pub struct EventContext<'a> {
    /// Exclusive access to the context-owned root and input transaction domain.
    window_manager: &'a mut WindowManager,
}

impl<'a> EventContext<'a> {
    /// Lends one already-exclusive window manager to an application dispatch transaction.
    pub(crate) fn new(window_manager: &'a mut WindowManager) -> Self {
        // Keep construction private so application code can only receive this capability at the
        // borrow-safe boundary established by Context::update_ui_state.
        Self { window_manager }
    }

    /// Creates an open retained window around one uniquely owned application node.
    ///
    /// The returned handle is weak; the parent [`Context`] remains the sole root owner.
    pub fn create_window(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        // Delegate to the same non-generic owner used by Context::create_window so event-time and
        // ordinary root construction have identical lifetime, z-order, and invalidation behavior.
        self.window_manager.create_window(name, rect, content)
    }

    /// Creates an independently positioned child window owned by `parent`.
    ///
    /// The ownership edge controls lifetime and inherited stacking policy; it does not clip or lay
    /// out the child. A stale or ineligible parent returns a checked root mutation error.
    pub fn create_child_window(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // Event-time construction follows the same checked ownership path as ordinary Context use.
        self.window_manager.create_child_window(parent, name, rect, content)
    }

    /// Creates a hidden retained dialog owned by `parent`.
    ///
    /// Show the returned root with [`Self::set_root_visible`]. A shown dialog becomes the active
    /// modal subtree before the layout immediately following this event dispatch.
    pub fn create_dialog(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // WindowManager validates and records the stable ownership edge before returning the handle.
        self.window_manager.create_dialog(parent, name, rect, content)
    }

    /// Creates a hidden auto-sized popup owned by `parent`.
    ///
    /// The operation is generic root construction; this capability has no knowledge of the button,
    /// combo, menu, or other application behavior that may later show the popup.
    pub fn create_popup(&mut self, parent: RootId, name: &str, content: Node) -> Result<PopupHandle, RootMutationError> {
        // Register the persistent tree through the popup-only construction path so later anchored
        // placement is statically restricted to roots carrying popup policy.
        self.window_manager.create_popup(parent, name, content)
    }

    /// Replaces a retained root title before the next layout commit.
    pub fn set_root_name(&mut self, root: RootId, name: impl Into<String>) -> Result<(), RootMutationError> {
        self.window_manager.set_root_name(root, name.into())
    }

    /// Replaces a retained root rectangle before the next layout commit.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        // Preserve WindowManager's checked root identity and active-chrome reconciliation.
        self.window_manager.set_root_rect(root, rect)
    }

    /// Replaces a retained root size without changing its origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        // Size is still authoritative root state rather than an application-side deferred value.
        self.window_manager.set_root_size(root, size)
    }

    /// Replaces the chrome options for a retained root.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        // Apply option-dependent capture cleanup in the shared WindowManager implementation.
        self.window_manager.set_root_options(root, options)
    }

    /// Assigns an ordinary window to one of the sixteen fixed application layers.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        // Layer validation, inherited-popup propagation, and modal restrictions remain one
        // WindowManager transaction at the event-safe mutation boundary.
        self.window_manager.set_root_layer(root, layer)
    }

    /// Returns the registered root's fixed, direct-parent inherited, or modal layer policy.
    pub fn root_layer_binding(&self, root: RootId) -> Result<LayerBinding, RootMutationError> {
        self.window_manager.root_layer_binding(root)
    }

    /// Shows or hides a retained root while preserving its tree and concrete widget state.
    ///
    /// Dialog activation and popup-branch dismissal use the same policy as
    /// [`Context::set_root_visible`]. Showing a popup through this generic operation is rejected
    /// because it cannot establish layer inheritance; use [`Self::show_popup`] or
    /// [`Self::show_popup_at`] instead. Hiding a popup remains supported.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        // Mutate the root synchronously at the safe dispatch boundary so the following layout sees
        // the requested visibility without an application-owned frame flag.
        self.window_manager.set_root_visible(root, visible)
    }

    /// Shows a popup at the current pointer position under its stable owner.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        self.window_manager.show_popup(popup)
    }

    /// Shows a popup at an exact screen-space anchor before the following layout commit.
    ///
    /// This atomic form is intended for composed controls such as menus and combos. It applies
    /// popup-branch replacement and replaces the popup rectangle, so no pointer-relative intermediate
    /// placement can be observed. Stable popup ownership determines whether ancestors are retained
    /// or a new top-level popup branch opens. The typed handle prevents passing a window or dialog
    /// root as the popup target.
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), RootMutationError> {
        // Delegate the complete transaction while retaining compile-time popup identity across the
        // event façade boundary.
        self.window_manager.show_popup_at(popup, anchor)
    }

    /// Raises a registered root inside its effective layer and reports whether it still exists.
    ///
    /// This operation never moves an ordinary root across another numeric application layer.
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool {
        // Let WindowManager preserve the active modal root above the requested ordinary root.
        self.window_manager.bring_root_to_front(root)
    }

    /// Permanently unregisters a root and drops its complete retained tree.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        // Root destruction also expires every weak widget and root handle owned by the removed tree.
        self.window_manager.destroy_root(root)
    }

    /// Returns the resolved UI style currently used by the owning context.
    pub fn style(&self) -> &Style {
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
/// Across ordinary roots, pointer hover and new presses follow topmost hit geometry. A press raises
/// its root only within the effective layer and records the ordinary `active_root` independently.
/// Drag remains confined by pointer capture or the front eligible visual root, while wheel input
/// follows the topmost eligible root under the pointer. Keyboard and text return to the captured
/// or active root. Captured pointer release still returns to its widget so local drag state is
/// cleaned up.
///
/// A visible dialog is modal. It occupies the dedicated band above all application layers and forms
/// the only eligible input group together with a popup that it initiates. Pointer input outside
/// that group is consumed at the cross-root boundary; other roots remain visible and continue to
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
/// [`crate::TypedWidgetHandle`] or [`RootHandle::widget`] access. Mutating layout-affecting state after the
/// last update commit makes that commit semantically stale; drop the unsubmitted frame and call
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
    /// Context-aware subscribers receive [`EventContext`] only at these boundaries; their root and
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
        // dispatch closure can then lend the manager back through EventContext without aliasing the
        // independently borrowed application dispatcher.
        let window_manager = &mut self.window_manager;
        let widget_event_dispatcher = &mut self.widget_event_dispatcher;
        window_manager.update_with(dimensions, &atlas, state, |window_manager, state| {
            // This closure runs only after complete retained traversals release widget borrows.
            let mut event_context = EventContext::new(window_manager);
            widget_event_dispatcher.dispatch_with_context(state, &mut event_context)
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
    /// The supplied [`EventContext`] exists only for one dispatch call after retained widget borrows
    /// have ended. Root mutations performed through it are committed by the layout immediately
    /// following that dispatch boundary. Use [`Context::subscribe`] when the handler only mutates
    /// application or widget state.
    pub fn subscribe_context<E: crate::WidgetEvent>(
        &mut self,
        port: crate::WidgetEventPortHandle<E>,
        method: for<'a> fn(&mut State, &mut EventContext<'a>, &E),
    ) -> Result<(), crate::SubscribeError> {
        // Store the typed function pointer in the same context-owned dispatcher as state-only
        // subscriptions; only its invocation adapter differs.
        self.widget_event_dispatcher.subscribe_context(port, method)
    }

    /// Subscribes a context-aware state method with one immutable bound application value.
    ///
    /// The bound value precedes [`EventContext`] and the event payload in the method signature,
    /// matching the established [`Context::subscribe_with`] argument order.
    pub fn subscribe_context_with<E: crate::WidgetEvent, BoundContext: 'static>(
        &mut self,
        port: crate::WidgetEventPortHandle<E>,
        context: BoundContext,
        method: for<'a> fn(&mut State, &BoundContext, &mut EventContext<'a>, &E),
    ) -> Result<(), crate::SubscribeError> {
        // The dispatcher owns the bound value and preserves ordinary subscription ordering.
        self.widget_event_dispatcher.subscribe_context_with(port, context, method)
    }
}

// Root lifecycle façade.

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Creates an open retained window around one uniquely owned application node.
    ///
    /// The returned handle is weak; `Context` owns the root until explicit destruction.
    pub fn create_window(&mut self, name: &str, rect: Recti, content: Node) -> RootHandle {
        self.window_manager.create_window(name, rect, content)
    }

    /// Creates an independently positioned child window owned by `parent`.
    ///
    /// Ownership controls recursive lifecycle and inherited stacking policy without introducing
    /// parent-relative layout or clipping.
    pub fn create_child_window(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // Keep parent validation and reverse-edge registration inside the sole root owner.
        self.window_manager.create_child_window(parent, name, rect, content)
    }

    /// Creates a hidden retained dialog owned by `parent`.
    ///
    /// Show it with [`Context::set_root_visible`]. A visible dialog enters the dedicated modal layer
    /// and its complete owned subtree becomes the active modal input group. Other roots remain
    /// input-ineligible until the dialog is hidden or destroyed.
    pub fn create_dialog(&mut self, parent: RootId, name: &str, rect: Recti, content: Node) -> Result<RootHandle, RootMutationError> {
        // The manager records ownership before exposing the weak dialog handle.
        self.window_manager.create_dialog(parent, name, rect, content)
    }

    /// Creates a hidden auto-sized popup owned by `parent`.
    ///
    /// Show it through [`Self::show_popup`] or [`Self::show_popup_at`]. Its stable parent determines
    /// inherited stacking, recursive lifetime, and whether it joins an active modal subtree. A
    /// popup occupies the transient tier above ordinary roots in its effective band without
    /// crossing a higher fixed layer. An outside press or competing popup request hides it and
    /// records a submission.
    pub fn create_popup(&mut self, parent: RootId, name: &str, content: Node) -> Result<PopupHandle, RootMutationError> {
        // Return the typed weak capability created by WindowManager so callers cannot request
        // anchored popup policy for an ordinary window or dialog identifier.
        self.window_manager.create_popup(parent, name, content)
    }

    /// Replaces a retained root title silently.
    pub fn set_root_name(&mut self, root: RootId, name: impl Into<String>) -> Result<(), RootMutationError> {
        self.window_manager.set_root_name(root, name.into())
    }

    /// Replaces a root rectangle silently while retaining any compatible captured chrome mode.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        self.window_manager.set_root_rect(root, rect)
    }

    /// Replaces a root size silently without changing its origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        self.window_manager.set_root_size(root, size)
    }

    /// Replaces root chrome options silently.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        self.window_manager.set_root_options(root, options)
    }

    /// Assigns an ordinary window to a fixed application layer in the inclusive range `0..=15`.
    ///
    /// Newly created independent windows use layer 15. Owned windows and popups inherit through
    /// their stable parent, and dialogs occupy the separate modal layer, so every owned root rejects
    /// direct layer assignment.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        self.window_manager.set_root_layer(root, layer)
    }

    /// Returns the registered root's current layer-binding policy.
    pub fn root_layer_binding(&self, root: RootId) -> Result<LayerBinding, RootMutationError> {
        self.window_manager.root_layer_binding(root)
    }

    /// Shows or hides a retained root, preserving its tree and concrete widget state.
    ///
    /// Showing a dialog records it as the most recently activated visible modal subtree. Hiding it
    /// restores the previous visible sibling dialog, if any; otherwise ordinary cross-root routing
    /// resumes. Showing a popup is rejected because generic visibility cannot reconcile the active
    /// popup branch; use [`Self::show_popup`] or [`Self::show_popup_at`]. Hiding a popup remains
    /// supported. Hiding any root also hides every owned descendant.
    ///
    /// This is distinct from [`Context::destroy_root`], which drops the complete retained owner.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        self.window_manager.set_root_visible(root, visible)
    }

    /// Shows a popup at the current pointer position under its stable owner.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        self.window_manager.show_popup(popup)
    }

    /// Shows a popup at an exact screen-space anchor in one checked root mutation.
    ///
    /// Use this for a popup whose position belongs to the semantic event that opened it. Use
    /// [`Self::show_popup`] when the current pointer position is the desired anchor. Both forms
    /// use the stable owner recorded at popup creation. A popup parent retains its ancestor branch;
    /// a window or dialog parent replaces the previous popup branch. The typed parameter makes an
    /// ordinary [`RootHandle`] ineligible for
    /// popup-only placement policy:
    ///
    /// ```compile_fail
    /// use microui_redux::prelude::*;
    ///
    /// fn cannot_anchor_window<B: RendererBackend, State: 'static>(
    ///     context: &mut Context<B, State>,
    ///     window: &RootHandle,
    ///     anchor: Recti,
    /// ) {
    ///     context.show_popup_at(window, anchor);
    /// }
    /// ```
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), RootMutationError> {
        // Keep popup identity typed through the public façade; the manager still checks whether the
        // weak root remains registered and whether its widget state can be borrowed atomically.
        self.window_manager.show_popup_at(popup, anchor)
    }

    /// Raises a registered root inside its effective layer and reports whether it exists.
    ///
    /// The operation cannot cross a numeric application-layer boundary, and the active modal
    /// dialog remains above every application root.
    pub fn bring_root_to_front(&mut self, root: RootId) -> bool {
        self.window_manager.bring_root_to_front(root)
    }

    /// Permanently unregisters a root and releases its complete retained tree.
    ///
    /// Destroying the active dialog restores the previous visible dialog, if any;
    /// otherwise ordinary cross-root routing resumes.
    ///
    /// There is intentionally no root-content replacement operation. Destroy and recreate a root
    /// to install a different root owner, or mutate descendants through their typed widget handles.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        self.window_manager.destroy_root(root)
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
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        self.window_manager.debug_rendered_root_names()
    }

    pub(crate) fn debug_root_zindex(&self, root: RootId) -> Option<i32> {
        self.window_manager.debug_root_zindex(root)
    }

    pub(crate) fn debug_root_layer_binding(&self, root: RootId) -> Option<LayerBinding> {
        self.window_manager.debug_root_layer_binding(root)
    }

    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        self.window_manager.debug_active_root()
    }

    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.window_manager.debug_modal_root()
    }

    pub(crate) fn debug_root_body(&self, root: RootId) -> Option<Recti> {
        self.window_manager.debug_root_body(root, &self.renderer.atlas())
    }

    pub(crate) fn debug_root_content_size(&self, root: RootId) -> Option<Dimensioni> {
        self.window_manager.debug_root_content_size(root)
    }

    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        self.window_manager.debug_root_runtime_metrics(root)
    }

    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        self.window_manager.debug_root_has_pointer_capture(root)
    }

    pub(crate) fn debug_root_node_count(&self, root: RootId) -> Option<usize> {
        self.window_manager.debug_root_node_count(root)
    }

    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.window_manager.debug_root_node_rect(root, node)
    }

    pub(crate) fn debug_root_chrome(&self, root: RootId) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
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
