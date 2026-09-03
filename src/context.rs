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

use crate::window_manager::{LayerBinding, PopupHandle, SurfaceCreationError, SurfaceMutationError, Window, WindowHandle, WindowManager, WindowOption};
#[cfg(test)]
use crate::window_manager::RootId;
use crate::render::{CustomRenderArgs, CustomRenderHandle, CustomRenderRegistryError, FrameInfo, RenderError, Renderer, RendererBackend};
use crate::{AtlasHandle, Dimensioni, ImageSource, KeyEvent, Menu, MouseButton, Node, Recti, ResourceCatalog, Skin, SkinBundle, TextureError, TextureId};
#[cfg(feature = "theme-json")]
use crate::{LoadedTheme, ThemeLoadError};
#[cfg(feature = "theme-json")]
use std::path::Path;

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
    /// Returns an owner-preserving [`SurfaceCreationError`] for a stale, foreign, or modal parent.
    /// [`SurfaceCreationError::reason`] distinguishes
    /// [`SurfaceMutationError::UnknownWindow`] from
    /// [`SurfaceMutationError::InvalidChildWindowParent`], and
    /// [`SurfaceCreationError::into_input`] recovers the unchanged `window` for retry or reuse.
    #[allow(clippy::result_large_err)] // Failure deliberately returns the complete unique Window without allocation or erasure.
    pub fn create_child_window(&mut self, parent: &WindowHandle, window: Window) -> Result<WindowHandle, SurfaceCreationError<Window>> {
        // WindowManager validates the complete parent contract before transferring the Window into
        // its forest, so every failure returns the exact retained tree supplied by the caller.
        self.window_manager.create_child_window(parent, window)
    }

    /// Creates a hidden retained dialog owned by `parent`.
    ///
    /// `parent` must be a live independent or structural child window from this Context. Show the
    /// returned dialog with [`Self::set_window_visible`]; it becomes the front modal group before
    /// the layout immediately following this UI transaction.
    ///
    /// # Errors
    ///
    /// Returns an owner-preserving [`SurfaceCreationError`] when `parent` is stale, foreign, or
    /// modal. Inspect [`SurfaceCreationError::reason`] and reclaim the original `window` with
    /// [`SurfaceCreationError::into_input`].
    #[allow(clippy::result_large_err)] // Failure deliberately returns the complete unique Window without allocation or erasure.
    pub fn create_dialog(&mut self, parent: &WindowHandle, window: Window) -> Result<WindowHandle, SurfaceCreationError<Window>> {
        // Authenticate and classify the parent before the manager consumes the dialog definition.
        self.window_manager.create_dialog(parent, window)
    }

    /// Creates a hidden auto-sized popup owned directly by `parent`.
    ///
    /// The popup definition and retained content tree become a concrete child in the same forest.
    /// The distinct handle prevents popup identity from entering window-only operations.
    ///
    /// # Errors
    ///
    /// Returns [`SurfaceCreationError<Node>`] when `parent` is stale or belongs to another
    /// Context. [`SurfaceCreationError::into_input`] recovers the unchanged content node.
    #[allow(clippy::result_large_err)] // Failure deliberately returns the complete unique Node without allocation or erasure.
    pub fn create_popup(&mut self, parent: &WindowHandle, name: &str, content: Node) -> Result<PopupHandle, SurfaceCreationError<Node>> {
        // Authenticate the owner capability before transferring the popup content into the forest.
        self.window_manager.create_popup(parent, name, content)
    }

    /// Creates a hidden popup menu owned directly by `parent`.
    ///
    /// The supplied [`Menu`] is the same concrete declaration used below a menu-bar heading. Its
    /// label names the retained popup for diagnostics; its items, separators, and submenus are
    /// presented by the normal compact menu surface when the returned handle is shown.
    ///
    /// # Errors
    ///
    /// Returns [`SurfaceCreationError<Menu>`] when `parent` is stale or belongs to another
    /// Context. [`SurfaceCreationError::into_input`] recovers the unchanged menu declaration.
    #[allow(clippy::result_large_err)] // Failure deliberately returns the complete unique Menu without allocation or erasure.
    pub fn create_menu_popup(&mut self, parent: &WindowHandle, menu: Menu) -> Result<PopupHandle, SurfaceCreationError<Menu>> {
        // Authenticate the owner capability before transferring the menu into the surface forest.
        self.window_manager.create_menu_popup(parent, menu)
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

    /// Replaces the presentation and interaction options for a retained window or dialog.
    pub fn set_window_options(&mut self, window: &WindowHandle, options: WindowOption) -> Result<(), SurfaceMutationError> {
        // Apply option-dependent capture cleanup and enabled-tree propagation in the concrete
        // WindowManager implementation rather than mirroring either policy in this façade.
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

    /// Shows a popup at the current pointer position and activates its keyboard scope.
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

    /// Shows and activates a popup at an exact screen-space anchor before the following layout commit.
    ///
    /// This atomic form is intended for composed controls such as combos. It applies popup-path
    /// replacement and replaces the popup rectangle, so no pointer-relative intermediate placement
    /// can be observed. Widget popups select their first eligible keyboard target after layout;
    /// popup menus enter their ordinary compact-menu keyboard scope. The typed handle prevents
    /// passing a window or dialog as the popup target.
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

    /// Returns the resolved UI skin currently used by the owning context.
    pub fn skin(&self) -> &Skin {
        // The manager owns the resolved skin used by layout, input presentation, and paint.
        self.window_manager.skin()
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
/// scope and records that concrete root or popup as the active surface independently. Drag remains
/// confined by pointer capture or the front eligible visual root, while wheel input follows the
/// topmost eligible root under the pointer. Keyboard and text return to persistent focus in the
/// active surface; pointer capture does not replace that focus. Captured pointer release still
/// returns to its widget so local drag state is cleaned up. An active menu uses the same surface
/// identity plus its concrete container's selected direct child; it temporarily consumes keyboard
/// and text while preserving the focused application widget that resumes after the menu closes.
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
/// mutation or another logical frame until it is rendered or cancelled. The diagnostic-matched
/// `tests/ui/context_mutate_during_frame.rs` contract test verifies that rejection beside a passing
/// complete frame lifecycle.
pub struct Context<B: RendererBackend, State: 'static = ()> {
    /// High-level renderer that replays root display lists.
    renderer: Renderer<B>,
    /// Immutable application fonts and icons from which every later theme is derived.
    resource_catalog: ResourceCatalog,
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
/// [`crate::TypedWidgetHandle`] access. Programmatic mutation after the last update commit may make
/// that commit semantically stale; drop the unsubmitted frame and call
/// [`Context::update_ui`] or [`Context::update_ui_state`] again before painting. No separate
/// Context token exists.
///
/// Submission consumes the frame, making a second submission unrepresentable. The
/// diagnostic-matched `tests/ui/context_frame_submit_twice.rs` contract test verifies the move
/// error rather than treating any unrelated compilation failure as success.
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
    ///
    /// # Panics
    ///
    /// Panics when the backend atlas lacks the `body` font or any semantic icon required by
    /// [`crate::IconRole::ALL`]. Every constructible atlas already has a validated white
    /// rendering tile.
    pub fn new(backend: B) -> Self {
        // Capture the renderer's initial resources before any replacement can occur. The default
        // active bundle and every later loaded theme derive from this same immutable catalog.
        let renderer = Renderer::new(backend);
        let resource_catalog = ResourceCatalog::new(renderer.atlas());
        let bundle = resource_catalog.default_skin_bundle();
        Self {
            renderer,
            resource_catalog,
            window_manager: WindowManager::new(bundle),
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

    /// Borrows the immutable application font and icon catalog captured at construction.
    ///
    /// Selecting another skin bundle never changes this value. Applications can therefore create
    /// checked exact [`crate::FontRef`] and [`crate::IconRef`] values here and keep their typed IDs
    /// in retained widget state across themes derived from this catalog.
    pub fn resource_catalog(&self) -> &ResourceCatalog {
        // Expose one-time named lookup without exposing the catalog's source atlas itself.
        &self.resource_catalog
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
    /// Runs eventless widget work, drains input, and commits layout for a polling-only context.
    #[track_caller]
    pub fn update_ui(&mut self, dimensions: Dimensioni) {
        assert!(dimensions.width > 0 && dimensions.height > 0, "update_ui dimensions must be positive");
        self.window_manager.update(dimensions);
    }
}

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Drains ordered input, dispatches typed retained UI events, and commits layout for `dimensions`.
    ///
    /// One synchronization layout always runs first. Each queued input event then causes exactly
    /// one route, one full eligible-tree update, and another layout commit. When the queue is empty,
    /// one eventless eligible-tree update and a follow-up layout consume pending programmatic work
    /// such as caret reveal. Geometry produced for one update is therefore authoritative when
    /// routing the next event. This method performs no timer synthesis, painting, or backend
    /// submission.
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
        // Split the two Context-owned transaction participants before entering WindowManager. The
        // dispatch closure can then lend the manager back through Ui without aliasing the
        // independently borrowed application dispatcher.
        let window_manager = &mut self.window_manager;
        let widget_event_dispatcher = &mut self.widget_event_dispatcher;
        window_manager.update_with(dimensions, state, |window_manager, state| {
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

    /// Queues one backend-normalized logical keyboard transition.
    ///
    /// Character keys identify shortcut candidates; composed UTF-8 input must still be forwarded
    /// separately through [`Self::text`]. The supplied modifier snapshot is authoritative for this
    /// transition and for later pointer updates until another key transition replaces it.
    pub fn key(&mut self, event: KeyEvent) {
        self.window_manager.key(event);
    }

    /// Queues one UTF-8 text transition, including an empty string.
    ///
    /// Each editor applies its documented storage policy: single-line controls remove line endings,
    /// while multiline controls normalize CR and CRLF to LF. Rendering remains limited to glyphs
    /// in the selected atlas font and substitutes underscore metrics for missing characters.
    pub fn text(&mut self, text: &str) {
        self.window_manager.text(text);
    }
}

// Skin, backend callbacks, and renderer resources.

impl<B: RendererBackend, State: 'static> Context<B, State> {
    /// Registers one backend-specific callback for retained custom-render nodes.
    ///
    /// The callback is observational with respect to retained application state, topology,
    /// interaction, and layout. It may mutate callback-private rendering caches, but using a
    /// captured [`TypedWidgetHandle`](crate::TypedWidgetHandle) to mutate retained UI during frame
    /// execution is a contract violation rather than a deferred-next-frame update.
    ///
    /// A callback written for another backend frame type cannot be registered, and the active frame
    /// borrow cannot escape one callback invocation. These negative guarantees are checked by the
    /// diagnostic-matched fixtures under `tests/ui/`; ordinary doctests are unsuitable because any
    /// unrelated compiler error would also make a `compile_fail` block appear successful.
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

    /// Replaces the current UI skin.
    ///
    /// Image-backed visuals must originate from this Context's current bundle atlas. Start from
    /// `context.skin().clone()` when changing scalar values.
    ///
    /// # Panics
    ///
    /// Panics when the skin contains any resource identity absent from the current atlas.
    pub fn set_skin(&mut self, skin: Skin) {
        // Re-pair the edited skin with the existing immutable atlas at the one validated boundary.
        // The renderer needs no upload because this operation cannot replace atlas pixels.
        let bundle = SkinBundle::new(self.window_manager.skin_bundle().atlas().clone(), skin);
        self.window_manager.set_skin_bundle(bundle);
    }

    /// Uploads and installs one complete skin bundle as a single context transaction.
    ///
    /// The backend receives the atlas before the retained manager publishes either projection. If
    /// upload fails, the previous bundle remains active and renderable.
    ///
    /// # Errors
    ///
    /// Returns [`crate::AtlasUploadError`] when the backend cannot install the bundle atlas.
    pub fn set_skin_bundle(&mut self, bundle: &SkinBundle) -> Result<(), crate::AtlasUploadError> {
        // Renderer replacement is the only fallible step. Publish the already validated pair only
        // after it succeeds so Context never exposes a half-installed skin or atlas.
        self.renderer.replace_atlas(bundle.atlas().clone())?;
        debug_assert!(self.renderer.atlas().ptr_eq(bundle.atlas()));
        self.window_manager.set_skin_bundle(bundle.clone());
        Ok(())
    }

    /// Installs the loaded theme's complete skin bundle as one transaction.
    ///
    /// The backend uploads the theme atlas before this Context publishes the new bundle. If upload
    /// fails, the previous bundle remains active. Theme artwork is baked into the bundle atlas, so
    /// no secondary texture transaction is required.
    ///
    /// # Errors
    ///
    /// Returns [`crate::AtlasUploadError`] when the backend cannot allocate, upload, or bind the
    /// replacement atlas. The current theme remains unchanged.
    ///
    #[cfg(feature = "theme-json")]
    pub fn set_theme(&mut self, theme: &LoadedTheme) -> Result<(), crate::AtlasUploadError> {
        // LoadedTheme contributes only a selection name around the same atomic runtime value.
        self.set_skin_bundle(theme.bundle())
    }

    /// Borrows the complete resolved skin and matching immutable atlas.
    pub fn skin_bundle(&self) -> &SkinBundle {
        // WindowManager owns the authoritative pair used by every retained phase.
        self.window_manager.skin_bundle()
    }

    /// Returns the resolved UI skin currently used by this context.
    pub fn skin(&self) -> &Skin {
        // Preserve the concise read API as a projection, never as an independently owned field.
        self.skin_bundle().skin()
    }

    /// Returns the immutable atlas capability supplied by this context's rendering backend.
    ///
    /// Applications use this handle to resolve named fonts/icons and to construct a compatible
    /// [`Skin`]. Frame execution and resource bookkeeping remain context-owned implementation
    /// details; custom GPU work continues through [`Context::register_custom_renderer`].
    pub fn atlas(&self) -> AtlasHandle {
        // Return the atlas from the same bundle as `skin()` rather than consulting a parallel
        // renderer cache. Successful installation keeps the renderer on this exact allocation.
        self.skin_bundle().atlas().clone()
    }

    /// Attempts to load an RGBA image into this Context and returns its [`TextureId`].
    ///
    /// Dimensions and byte length are validated before an id is allocated; no failed operation
    /// records texture state in the Context.
    ///
    /// # Errors
    ///
    /// Returns a concrete [`TextureError`] that distinguishes invalid image data, identifier
    /// exhaustion, and backend creation or upload failure.
    pub fn try_load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> Result<TextureId, TextureError> {
        self.renderer.try_load_texture_rgba(width, height, pixels)
    }

    /// Loads an RGBA image into this Context and returns its [`TextureId`].
    ///
    /// Prefer [`Context::try_load_image_rgba`] when callers can handle upload failure.
    ///
    /// # Panics
    ///
    /// Panics when the RGBA dimensions or byte length are invalid, the Context has exhausted its
    /// texture identifier space, or the backend rejects texture creation or upload.
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
    ///
    /// # Errors
    ///
    /// Image decoding and validation failures remain available as [`TextureError::Image`],
    /// exhausted Context identifiers use [`TextureError::IdentifierSpaceExhausted`], and backend
    /// creation or upload failures use [`TextureError::Backend`]. A failure never records a
    /// partially created texture in the Context.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, TextureError> {
        match source {
            ImageSource::Raw { width, height, pixels } => self.try_load_image_rgba(width, height, pixels),
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, colors) = crate::image::load_image_bytes(ImageSource::Png { bytes })?;
                // Image loading has already proved each dimension fits the runtime's i32
                // coordinate domain, so these casts preserve the validated values exactly.
                let width = width as i32;
                let height = height as i32;
                let rgba: Vec<u8> = colors.into_iter().flat_map(|color| [color.x, color.y, color.z, color.w]).collect();
                self.try_load_image_rgba(width, height, rgba.as_slice())
            }
        }
    }

    /// Loads a versioned JSON theme and bakes every assigned image patch into its immutable atlas.
    ///
    /// Relative image paths are resolved against the JSON file's directory. Missing patch entries
    /// remain concrete flat-color patches derived from the document's palette. Successfully
    /// Each unique image path becomes one atlas region, allowing callers to retain several
    /// [`LoadedTheme`] values and switch their complete skin bundles safely.
    ///
    /// # Errors
    ///
    /// Returns [`ThemeLoadError`] for definition I/O, strict JSON schema errors, invalid role or
    /// slice data, font/artwork atlas construction, or image decoding. Loading never mutates the live
    /// backend; only a later [`Context::set_theme`] uploads the completed atlas transactionally.
    #[cfg(feature = "theme-json")]
    pub fn load_theme_file(&mut self, path: impl AsRef<Path>) -> Result<LoadedTheme, ThemeLoadError> {
        // The loader owns every construction stage and returns only the complete immutable bundle.
        // Applications can preload several values and choose one later without repeated file I/O.
        crate::theme::loader::load(path.as_ref(), self.resource_catalog.atlas())
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
        // The manager retains the exact body snapshot consumed by application layout.
        self.window_manager.debug_root_body(root)
    }

    /// Returns the committed framed client rectangle for internal chrome tests.
    pub(crate) fn debug_root_client(&self, root: RootId) -> Option<Recti> {
        // Client geometry lets tests distinguish full-width chrome from padded application content.
        self.window_manager.debug_root_client(root)
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
        self.window_manager.debug_root_chrome(root)
    }

    /// Returns optional caption and per-axis resize rectangles for internal window tests.
    pub(crate) fn debug_root_chrome_controls(&self, root: RootId) -> Option<crate::window_manager::DebugRootChromeControls> {
        // Use the renderer atlas for the same title-height calculation as committed layout.
        self.window_manager.debug_root_chrome_controls(root)
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
    /// exists for these dimensions, raw input is pending, or an invalidating typed mutation dirtied
    /// a visible widget tree. Specialized measurement-preserving setters may leave the current
    /// commit renderable while derived state waits for the next update. This operation is
    /// paint-only: it does not route input, update
    /// semantic state, run layout, synthesize timers, or produce a generic
    /// frame-result/resource-state object. Widget paint is observational with respect to
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
        self.context.window_manager.paint(self.info.dimensions());
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

#[cfg(all(test, feature = "theme-json", feature = "save-to-rust"))]
mod theme_tests {
    //! Context-level theme upload transaction tests.

    use super::*;
    use crate::render::{AtlasUploadError, FrameError, RendererFrame, TextureError, Vertex};
    use crate::test_support::{recording_backend, test_atlas};
    use std::fs;

    /// Minimal backend that can deterministically accept or reject atlas publication.
    struct ThemeAtlasBackend {
        /// Atlas returned to a newly constructed Context and replaced after successful uploads.
        atlas: AtlasHandle,
        /// Whether the next replacement should fail without changing `atlas`.
        reject_replacement: bool,
    }

    /// Empty frame used because these tests exercise resource mutation between frames only.
    #[must_use]
    struct ThemeAtlasFrame;

    impl RendererFrame for ThemeAtlasFrame {
        /// Ignores atlas-backed quads because no test frame is rendered.
        fn push_quad(&mut self, _vertices: [Vertex; 4]) {}

        /// Ignores atlas-backed triangles because no test frame is rendered.
        fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}

        /// Has no buffered work to flush.
        fn flush(&mut self) {}

        /// Ignores external textures because no test frame is rendered.
        fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
    }

    impl RendererBackend for ThemeAtlasBackend {
        type Frame<'a> = ThemeAtlasFrame;

        /// Returns the currently published atlas capability.
        fn get_atlas(&self) -> AtlasHandle {
            self.atlas.clone()
        }

        /// Models the required all-or-nothing backend replacement contract.
        fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), AtlasUploadError> {
            if self.reject_replacement {
                return Err(AtlasUploadError::new("fixture rejected atlas"));
            }
            self.atlas = atlas;
            Ok(())
        }

        /// Provides an inert frame for completeness of the backend contract.
        fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
            Ok(ThemeAtlasFrame)
        }

        /// Accepts validated theme PNGs without retaining native resources.
        fn create_texture(&mut self, _id: TextureId, _pixels: &[u8]) -> Result<(), TextureError> {
            Ok(())
        }

        /// Has no native texture allocation to release.
        fn destroy_texture(&mut self, _id: TextureId) {}
    }

    /// Encodes one small opaque RGBA PNG for a temporary theme fixture.
    fn fixture_png(width: u32, height: u32) -> Vec<u8> {
        // Use the same png crate version as the production decoder while keeping the fixture fully
        // generated and independent of repository assets.
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("fixture PNG header must encode");
            let pixels = vec![0xFF; width as usize * height as usize * 4];
            writer.write_image_data(pixels.as_slice()).expect("fixture PNG pixels must encode");
        }
        bytes
    }

    /// Verifies a failed theme load never mutates backend texture state.
    #[test]
    fn failed_theme_load_leaves_backend_textures_untouched() {
        let directory = tempfile::tempdir().expect("temporary theme directory must be available");
        fs::write(directory.path().join("button.png"), fixture_png(3, 3)).expect("fixture PNG must be writable");
        fs::write(
            directory.path().join("theme.json"),
            r#"{
                "schema_version": 1,
                "name": "Rollback",
                "appearances": {
                    "control": {
                        "button": {
                            "insets": { "left": 1, "top": 1, "right": 1, "bottom": 1 },
                            "enabled": { "normal": { "patch": { "type": "image", "path": "button.png" } } }
                        },
                        "checkbox": {
                            "enabled": { "normal": { "patch": { "type": "image", "path": "absent.png" } } }
                        }
                    }
                }
            }"#,
        )
        .expect("fixture JSON must be writable");
        // Leave enough atlas capacity that the fixture reaches asset I/O before packing can fail;
        // this isolates the intended missing-file error while retaining CPU-local rollback.
        let source = crate::atlas::builder::Builder::from_atlas_with_size(&test_atlas(), 64, 64)
            .expect("expanded fixture resources must fit")
            .build()
            .expect("expanded fixture atlas must validate");
        let (backend, log) = recording_backend(source);
        let mut context = Context::<_>::new(backend);
        log.clear();

        let error = match context.load_theme_file(directory.path().join("theme.json")) {
            Ok(_) => panic!("missing second image must fail atlas construction"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            ThemeLoadError::AtlasBuild {
                source: crate::atlas::builder::BuilderError::Asset { source },
            } if source.kind() == std::io::ErrorKind::NotFound && source.to_string().contains("absent.png")
        ));
        assert!(
            log.snapshot().is_empty(),
            "theme decoding and atlas construction must remain CPU-local until selection"
        );
    }

    /// Verifies a successful selection publishes one matching skin bundle to the Context.
    #[test]
    fn set_theme_replaces_atlas_and_skin_together() {
        let initial = test_atlas();
        let replacement = test_atlas();
        let replacement_style = Skin::from_atlas(&replacement);
        let replacement_font = replacement_style.resolve_font_role(&replacement, crate::FontRole::Body);
        let theme = LoadedTheme::new("Replacement", SkinBundle::new(replacement.clone(), replacement_style));
        let mut context = Context::<ThemeAtlasBackend>::new(ThemeAtlasBackend {
            atlas: initial.clone(),
            reject_replacement: false,
        });

        context.set_theme(&theme).expect("fixture backend must accept replacement atlas");

        assert!(context.atlas().ptr_eq(&replacement));
        assert!(!context.atlas().ptr_eq(&initial));
        assert_eq!(context.skin().resolve_font_role(&context.atlas(), crate::FontRole::Body), replacement_font);
    }

    /// Verifies an atlas upload error leaves both sides of the active theme pair unchanged.
    #[test]
    fn set_theme_failure_preserves_previous_atlas_and_skin() {
        let initial = test_atlas();
        let initial_font = Skin::from_atlas(&initial).resolve_font_role(&initial, crate::FontRole::Body);
        let replacement = test_atlas();
        let theme = LoadedTheme::new("Rejected", SkinBundle::new(replacement.clone(), Skin::from_atlas(&replacement)));
        let mut context = Context::<ThemeAtlasBackend>::new(ThemeAtlasBackend {
            atlas: initial.clone(),
            reject_replacement: true,
        });

        let error = context.set_theme(&theme).expect_err("fixture backend must reject replacement atlas");

        assert_eq!(error, AtlasUploadError::new("fixture rejected atlas"));
        assert!(context.atlas().ptr_eq(&initial));
        assert_eq!(context.skin().resolve_font_role(&context.atlas(), crate::FontRole::Body), initial_font);
    }

    /// Verifies consecutive theme loads always derive from the pristine application catalog.
    #[test]
    fn consecutive_theme_loads_do_not_copy_private_assets_from_the_active_theme() {
        let directory = tempfile::tempdir().expect("temporary theme directory must be available");
        fs::write(directory.path().join("first.png"), fixture_png(2, 2)).expect("first fixture PNG must be writable");
        fs::write(directory.path().join("second.png"), fixture_png(2, 2)).expect("second fixture PNG must be writable");
        fs::write(
            directory.path().join("first.json"),
            r#"{
                "schema_version": 1,
                "name": "First",
                "appearances": {
                    "control": { "button": { "enabled": { "normal": { "patch": { "type": "image", "path": "first.png" } } } } }
                }
            }"#,
        )
        .expect("first fixture JSON must be writable");
        fs::write(
            directory.path().join("second.json"),
            r#"{
                "schema_version": 1,
                "name": "Second",
                "appearances": {
                    "control": { "checkbox": { "enabled": { "normal": { "patch": { "type": "image", "path": "second.png" } } } } }
                }
            }"#,
        )
        .expect("second fixture JSON must be writable");

        // Expand the compact shared source so each derived theme has room for one artwork tile.
        // The catalog must retain this allocation even after the backend publishes the first one.
        let source = crate::atlas::builder::Builder::from_atlas_with_size(&test_atlas(), 64, 64)
            .expect("expanded application resources must fit")
            .build()
            .expect("expanded application resources must validate");
        let mut context = Context::<ThemeAtlasBackend>::new(ThemeAtlasBackend {
            atlas: source.clone(),
            reject_replacement: false,
        });
        let first = context
            .load_theme_file(directory.path().join("first.json"))
            .expect("first theme must load from the catalog");
        context.set_theme(&first).expect("first theme atlas must publish");
        let second = context
            .load_theme_file(directory.path().join("second.json"))
            .expect("second theme must ignore the active theme's private artwork");

        assert!(context.resource_catalog.atlas().ptr_eq(&source));
        assert!(!context.atlas().ptr_eq(&source));
        assert_eq!(
            second
                .bundle()
                .atlas()
                .clone_icon_table()
                .iter()
                .filter(|(name, _)| name.starts_with("@microui-theme-image/"))
                .count(),
            1,
            "the second atlas must contain only its own private theme image"
        );
    }
}
