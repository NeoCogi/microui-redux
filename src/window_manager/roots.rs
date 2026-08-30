//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Concrete retained surface ownership and cross-surface traversal policy.

use std::{cell::RefCell, fmt, rc::Rc};

use super::*;
use crate::menu::{MenuAction, MenuEntry, MenuSlot, MenuSurface};
use crate::{MouseButton, Node, UiInputEvent, Vec2i, rect};

use super::root_chrome::{RootChromeGeometry, RootChromePart, RootInteraction, record_root_background, record_root_overlay, root_chrome_geometry};

/// Failure reported by a checked window or popup mutation.
///
/// Window chrome and popup definitions are owned directly by the window manager, so failures describe
/// only stale identity or explicit ownership and layer policy. No application borrow can make one of
/// these mutations partially succeed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SurfaceMutationError {
    /// The supplied [`WindowHandle`] is stale or belongs to another Context.
    UnknownWindow,
    /// The supplied [`PopupHandle`] no longer identifies a window-owned popup definition.
    UnknownPopup,
    /// A dialog owner is hidden when shown or is not an ordinary window.
    InvalidDialogOwner,
    /// An authenticated child-window parent is modal or otherwise unable to own ordinary children.
    InvalidChildWindowParent,
    /// The requested fixed layer is outside the supported inclusive range `0..=15`.
    InvalidLayer(u8),
    /// The requested window occupies a manager-controlled child or modal layer.
    ManagedLayer,
    /// A popup owner is hidden, blocked by a modal, or missing from the active parent path.
    InvalidPopupParent,
}

/// Observable lifecycle event emitted by one application-authored popup.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PopupEvent {
    /// Popup policy removed this surface from the sole active branch.
    Dismissed,
}

impl crate::WidgetEvent for PopupEvent {}

/// Process-unique identifier for one popup definition inside its owning window.
///
/// The identifier is intentionally private and allocated independently from lifecycle events.
/// Application code proves popup identity by carrying a [`PopupHandle`], so popup definitions
/// cannot be passed to generic window APIs.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(super) struct PopupId(
    /// Shared non-reused value hidden behind the popup-specific type boundary.
    crate::identity::RetainedObjectId,
);

impl PopupId {
    /// Allocates one popup key that cannot collide across Contexts or later object lifetimes.
    fn allocate() -> Self {
        // Menu and application popups share this concrete wrapper because both occupy popup nodes;
        // only application popups expose a handle and event endpoint.
        Self(crate::identity::RetainedObjectId::allocate())
    }
}

/// Cloneable non-owning capability for one application-authored popup.
///
/// The private stable ID selects popup mutations; the weak typed endpoint observes dismissal. The
/// two values describe the same registered popup but neither derives identity from the other's
/// allocation, so allocator address reuse cannot retarget a stale capability.
pub struct PopupHandle {
    /// Process-unique concrete identity used only by the owning window manager.
    id: PopupId,
    /// Weak endpoint for policy-driven dismissal observations from this same popup.
    events: crate::WidgetEventPortHandle<PopupEvent>,
}

impl PopupHandle {
    /// Creates an application capability from an independently allocated identity and endpoint.
    fn new(id: PopupId, events: crate::WidgetEventPortHandle<PopupEvent>) -> Self {
        // Registration establishes this pairing once while it owns both concrete values.
        Self { id, events }
    }

    /// Returns the private concrete key used by popup forest traversal.
    pub(super) const fn id(&self) -> PopupId {
        // The typed value is copied directly without inspecting the weak event allocation.
        self.id
    }

    /// Returns this popup's weak typed dismissal endpoint.
    pub fn events(&self) -> crate::WidgetEventPortHandle<PopupEvent> {
        // Subscription receives only the weak endpoint; object selection remains a separate ID
        // operation performed by checked Ui methods.
        self.events.clone()
    }
}

impl Clone for PopupHandle {
    /// Clones the application capability without allocating identity or retaining the popup.
    fn clone(&self) -> Self {
        // Preserve the original stable ID/endpoint association in every application-held clone.
        Self { id: self.id, events: self.events.clone() }
    }
}

impl fmt::Debug for PopupHandle {
    /// Reports endpoint liveness without exposing the private process-local identifier.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug output intentionally describes only the observable endpoint state.
        f.debug_struct("PopupHandle").field("events", &self.events).finish_non_exhaustive()
    }
}

/// Storage and traversal state shared by a window body or popup body.
///
/// This is a value nested directly in its owner; it is not a registry or a polymorphic root. The
/// small common record keeps measurement and painting identical without duplicating window and
/// popup layout code.
struct Surface {
    /// Diagnostic name and optional window title text.
    name: String,
    /// Frame, padding, and automatic-size policy.
    options: WindowOption,
    /// Authoritative outer rectangle in screen coordinates.
    rect: Recti,
    /// Chrome and application-body geometry from the latest layout commit.
    geometry: RootChromeGeometry,
    /// Effective screen-space clip inherited from the viewport and structural window ancestors.
    ///
    /// Keeping this snapshot beside geometry lets manager chrome, compact menus, retained content,
    /// and cross-window hit testing consume the same committed visibility boundary.
    clip: Recti,
    /// Concrete content body with no erased node or dynamic downcast.
    body: SurfaceBody,
}

/// Concrete content variants owned by a forest surface.
enum SurfaceBody {
    /// Ordinary application-authored retained widget tree.
    Widgets {
        /// Sole application-authored root node owned by this surface.
        root: Node,
        /// Traversal state paired exclusively with `root` for its mounted lifetime.
        runtime: UiRuntime,
    },
    /// Compact manager-owned menu popup surface.
    Menu(MenuSurface),
}

impl Surface {
    /// Creates a surface around one uniquely owned application node.
    fn new(name: String, options: WindowOption, rect: Recti, content: Node) -> Self {
        // Geometry begins empty and is replaced before a visible surface can receive input or paint.
        Self {
            name,
            options,
            rect,
            geometry: RootChromeGeometry::default(),
            clip: Recti::default(),
            body: SurfaceBody::Widgets { root: content, runtime: UiRuntime::new() },
        }
    }

    /// Creates an auto-sized popup around one concrete menu surface.
    fn menu(surface: MenuSurface) -> Self {
        // Compact menu rows paint edge-to-edge inside their frame. Keep generic root padding out of
        // both top-level menus and recursive submenus while retaining the shared frame treatment.
        Self {
            name: String::new(),
            options: WindowOption::FRAME | WindowOption::NO_PADDING | WindowOption::NO_TITLE | WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE,
            rect: Recti::default(),
            geometry: RootChromeGeometry::default(),
            clip: Recti::default(),
            body: SurfaceBody::Menu(surface),
        }
    }

    /// Measures automatic axes and lays out the application tree in the derived body.
    fn layout(&mut self, menu_bar: Option<&mut MenuSurface>, style: &Style, atlas: &crate::AtlasHandle, clip: Recti) {
        // Commit the inherited surface boundary before any concrete body sees it. Every later
        // manager-owned hit or paint path reads this same snapshot until the next complete layout.
        self.clip = clip;
        let auto_width = self.options.intersects(WindowOption::AUTO_WIDTH);
        let auto_height = self.options.intersects(WindowOption::AUTO_HEIGHT);
        let mut menu_bar = menu_bar;
        let menu_size = menu_bar.as_deref_mut().map(|bar| bar.measure(style, atlas)).unwrap_or_default();
        if self.options.intersects(WindowOption::AUTO_SIZE) {
            // Convert retained outer bounds to application-body bounds before asking the content
            // tree for intrinsic size. Fixed axes retain their programmed outer extent.
            let shell = root_chrome_geometry(self.rect, Dimensioni::default(), &self.name, self.options, style, atlas);
            let horizontal_chrome = self.rect.width.saturating_sub(shell.body.width);
            let vertical_chrome = self.rect.height.saturating_sub(shell.body.height);
            let constraints = crate::Constraints::new(
                if auto_width {
                    crate::AvailableSpace::Unbounded
                } else {
                    crate::AvailableSpace::bounded(self.rect.width).shrink(horizontal_chrome)
                },
                if auto_height {
                    crate::AvailableSpace::Unbounded
                } else {
                    crate::AvailableSpace::bounded(self.rect.height).shrink(vertical_chrome)
                },
            );
            let body_constraints = crate::Constraints::new(constraints.width, constraints.height.shrink(menu_size.height));
            let child = self.body.measure(style, atlas, body_constraints);
            let combined = Dimensioni::new(child.width.max(menu_size.width), child.height.saturating_add(menu_size.height));
            let intrinsic = root_chrome_geometry(Recti::default(), combined, &self.name, self.options, style, atlas).intrinsic_outer;
            if auto_width {
                self.rect.width = intrinsic.width;
            }
            if auto_height {
                self.rect.height = intrinsic.height;
            }
        }

        // Store one geometry snapshot shared by application layout, chrome hit testing, and paint.
        self.geometry = root_chrome_geometry(self.rect, Dimensioni::default(), &self.name, self.options, style, atlas);
        let bar_height = menu_size.height.min(self.geometry.body.height.max(0));
        if let Some(bar) = menu_bar {
            bar.layout(
                Recti::new(self.geometry.body.x, self.geometry.body.y, self.geometry.body.width, bar_height),
                clip,
            );
        }
        let body = Recti::new(
            self.geometry.body.x,
            self.geometry.body.y.saturating_add(bar_height),
            self.geometry.body.width,
            self.geometry.body.height.saturating_sub(bar_height).max(0),
        );
        // Public/debug body geometry denotes application content, excluding the intrinsic menu bar.
        self.geometry.body = body;
        self.body.layout(style, atlas, body, clip);
    }

    /// Returns whether the outer surface contains one screen-space point.
    fn contains(&self, point: Vec2i) -> bool {
        // A structural child can overlap its parent chrome geometrically while remaining clipped
        // out of that area. Require both committed boundaries so input matches recorded pixels.
        self.rect.contains(&point) && self.clip.contains(&point)
    }
}

impl SurfaceBody {
    /// Measures either concrete body without type erasure.
    fn measure(&mut self, style: &Style, atlas: &crate::AtlasHandle, constraints: crate::Constraints) -> Dimensioni {
        match self {
            Self::Widgets { root, runtime } => runtime.measure_tree_root(root, style, atlas, constraints),
            Self::Menu(menu) => menu.measure(style, atlas),
        }
    }

    /// Commits one body allocation through the concrete variant.
    fn layout(&mut self, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, viewport: Recti) {
        match self {
            Self::Widgets { root, runtime } => runtime.layout_tree_root(root, style, atlas.clone(), rect, viewport),
            Self::Menu(menu) => menu.layout(rect, viewport),
        }
    }

    /// Returns whether this body owns pointer capture.
    fn has_capture(&self) -> bool {
        match self {
            Self::Widgets { runtime, .. } => runtime.has_pointer_capture(),
            Self::Menu(menu) => menu.has_capture(),
        }
    }

    /// Clears pointer state while preserving application keyboard focus where applicable.
    fn clear_pointer_targets(&mut self) {
        match self {
            Self::Widgets { runtime, .. } => {
                // Manager-owned overlays may preempt an application gesture while preserving the
                // independent keyboard-focus identity retained by the application runtime.
                runtime.clear_pointer_targets();
            }
            Self::Menu(menu) => menu.clear_pointer_targets(),
        }
    }

    /// Clears every transient input identity through the concrete body variant.
    fn clear_transient_targets(&mut self) {
        match self {
            Self::Widgets { runtime, .. } => runtime.clear_transient_targets(),
            Self::Menu(menu) => menu.clear_pointer_targets(),
        }
    }

    /// Returns the concrete menu body when this is a private menu popup.
    fn menu(&self) -> Option<&MenuSurface> {
        match self {
            Self::Menu(menu) => Some(menu),
            Self::Widgets { .. } => None,
        }
    }

    /// Returns the mutable concrete menu body when this is a private menu popup.
    fn menu_mut(&mut self) -> Option<&mut MenuSurface> {
        match self {
            Self::Menu(menu) => Some(menu),
            Self::Widgets { .. } => None,
        }
    }

    /// Starts an update cycle for widget bodies; concrete menus have no traversal runtime.
    fn begin_update(&mut self) {
        if let Self::Widgets { runtime, .. } = self {
            runtime.begin_update();
        }
    }

    /// Stages event-local routing only for a retained widget tree.
    fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        if let Self::Widgets { runtime, .. } = self {
            runtime.begin_input_event(pointer_input_enabled, event);
        }
    }

    /// Routes drag continuation or release to the captured application node, if this is a widget body.
    fn route_captured_pointer(&mut self, style: &Style, mouse_buttons: MouseButton, event: &UiInputEvent) -> Option<bool> {
        let Self::Widgets { root, runtime } = self else {
            return None;
        };
        runtime.route_captured_pointer_input_event(std::slice::from_mut(root), style, mouse_buttons, event)
    }

    /// Returns whether a widget runtime accepts an ordinary pointer hit for the staged event.
    fn accepts_pointer_input(&self) -> bool {
        matches!(self, Self::Widgets { runtime, .. } if runtime.accepts_pointer_input())
    }

    /// Routes one ordinary widget hit and records any resulting capture transition.
    fn route_pointer(&mut self, style: &Style, event: &UiInputEvent, mouse_buttons: MouseButton) -> Option<crate::ui_node::RuntimeNodeId> {
        let Self::Widgets { root, runtime } = self else {
            return None;
        };
        // Capture changes only after the selected target and its ancestors classify the event.
        let (owner, result) = runtime.route_input_event_to_node_ref(root, style, event)?;
        runtime.update_pointer_capture(owner, result, event, mouse_buttons);
        Some(owner)
    }

    /// Routes keyboard/text input only to application widget bodies.
    fn route_focus(&mut self, style: &Style, event: &UiInputEvent) {
        if let Self::Widgets { root, runtime } = self {
            runtime.route_focus_input_event(std::slice::from_mut(root), style, event);
        }
    }

    /// Advances focus inside a retained widget body and reports whether it contains a Tab stop.
    fn advance_focus(&mut self, reverse: bool) -> bool {
        let Self::Widgets { root, runtime } = self else {
            return false;
        };
        runtime.advance_focus(std::slice::from_mut(root), reverse)
    }

    /// Updates widget bodies; menu state is committed synchronously by direct pointer routing.
    fn update(&mut self, style: &Style, atlas: crate::AtlasHandle, input: crate::input::InputSnapshot) {
        if let Self::Widgets { root, runtime } = self {
            runtime.update_tree_root(root, style, atlas, input);
        }
    }

    /// Paints one concrete body into the shared manager display list.
    fn paint(&mut self, display_list: &mut crate::render::DisplayList, style: &Style, atlas: &crate::AtlasHandle, focus_visible: bool) {
        match self {
            Self::Widgets { root, runtime } => runtime.paint_tree_root(root, display_list, style, atlas.clone(), focus_visible),
            Self::Menu(menu) => menu.paint(display_list, style, atlas),
        }
    }

    /// Resolves one retained widget rectangle for relational popup anchors and tests.
    #[cfg(test)]
    fn node_rect(&self, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        let Self::Widgets { root, runtime } = self else {
            return None;
        };
        // Runtime identities are process-unique, so the first recursive match is authoritative.
        runtime.node_rect(std::slice::from_ref(root), node)
    }

    /// Returns committed widget content size for retained layout tests.
    #[cfg(test)]
    fn content_size(&self) -> Option<Dimensioni> {
        match self {
            Self::Widgets { runtime, .. } => Some(runtime.debug_root_content_size()),
            Self::Menu(_) => None,
        }
    }

    /// Returns widget traversal counters for phase-order tests.
    #[cfg(test)]
    fn metrics(&self) -> Option<crate::ui_node::RuntimeMetrics> {
        match self {
            Self::Widgets { runtime, .. } => Some(runtime.debug_metrics()),
            Self::Menu(_) => None,
        }
    }

    /// Counts application-authored nodes without manager chrome.
    #[cfg(test)]
    fn node_count(&self) -> Option<usize> {
        match self {
            Self::Widgets { root, .. } => Some(root.debug_node_count()),
            Self::Menu(_) => None,
        }
    }
}

/// Private identity for one node in the concrete surface forest.
///
/// Public code carries stable [`WindowHandle`] and [`PopupHandle`] capabilities. Internally, this
/// compact process-unique typed key is sufficient for common layout, input, and paint traversal
/// without an erased payload, trait object, or `(owner, popup)` adapter pair.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum SurfaceKey {
    /// An independent window, structural child window, or modal dialog.
    Root(RootId),
    /// A window- or popup-owned transient surface.
    Popup(PopupId),
}

/// Layer policy stored only on root nodes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RootMode {
    /// Independent application window in one caller-selected fixed layer.
    Normal {
        /// Numeric stacking layer in the public fixed range.
        layer: u8,
    },
    /// Ordinary window structurally nested below another independent or child window.
    ///
    /// A child has no independent global layer. Its family root supplies the fixed band, while its
    /// position in forest chronology supplies order among siblings with the same direct parent.
    Child,
    /// Dialog in the manager-owned modal layer.
    ///
    /// Its ordinary owner is represented by [`SurfaceNode::parent`], not duplicated here.
    Modal,
}

/// Root-specific policy attached to a concrete surface node.
struct RootState {
    /// Independent fixed-layer, structurally inherited child, or manager-owned modal policy.
    mode: RootMode,
    /// Local visibility intent for this root.
    ///
    /// A child or dialog participates only when this bit and every ancestor's bit are true. Keeping
    /// the local value lets children recover after a temporarily hidden parent becomes visible.
    visible: bool,
    /// Manager-owned title movement or resize gesture.
    interaction: RootInteraction,
    /// Strong owner of the unified window geometry/lifecycle event stream.
    events: Rc<RefCell<crate::event::WidgetEventPort<WindowEvent>>>,
    /// Optional concrete menu bar laid out above the application widget body.
    menu_bar: Option<MenuSurface>,
    /// Descendant viewport policy applied when layout enters direct structural child windows.
    child_window_clip: ChildWindowClip,
}

/// Popup-specific policy attached to a concrete surface node.
enum PopupState {
    /// Application-authored popup with exact placement and an observable dismissal event.
    Application {
        /// Exact screen rectangle supplied by the public popup API.
        anchor: Recti,
        /// Strong owner of policy-driven popup lifecycle events.
        events: Rc<RefCell<crate::event::WidgetEventPort<PopupEvent>>>,
        /// Whether the next committed layout must select this popup's first keyboard target.
        focus_on_layout: bool,
    },
    /// Private menu popup positioned from one slot in its direct forest parent.
    Menu {
        /// Heading or parent-row index that opens and anchors this popup.
        trigger_slot: usize,
    },
}

/// Concrete role data for a retained surface node.
enum SurfaceKind {
    /// Window or dialog policy.
    Root(RootState),
    /// Transient popup policy.
    Popup(PopupState),
}

/// One retained surface and its sole structural parent edge.
///
/// Independent roots have no parent, child windows and dialogs point to their ordinary owner,
/// top-level popups point to a root, and submenu popups point to their parent popup. This is the
/// only ownership relation stored by the manager; the owning root of any descendant is derived by
/// following it.
struct SurfaceNode {
    /// Stable concrete identity of this node.
    key: SurfaceKey,
    /// Sole owner edge, always directed toward an already retained ancestor.
    parent: Option<SurfaceKey>,
    /// Geometry and uniquely owned application tree shared by both concrete roles.
    surface: Surface,
    /// Role-specific policy with no type erasure.
    kind: SurfaceKind,
}

impl SurfaceNode {
    /// Returns root policy when this node is a window or dialog.
    fn root(&self) -> Option<&RootState> {
        match &self.kind {
            SurfaceKind::Root(root) => Some(root),
            SurfaceKind::Popup(_) => None,
        }
    }

    /// Returns mutable root policy when this node is a window or dialog.
    fn root_mut(&mut self) -> Option<&mut RootState> {
        match &mut self.kind {
            SurfaceKind::Root(root) => Some(root),
            SurfaceKind::Popup(_) => None,
        }
    }

    /// Returns popup policy when this node is transient.
    fn popup(&self) -> Option<&PopupState> {
        match &self.kind {
            SurfaceKind::Popup(popup) => Some(popup),
            SurfaceKind::Root(_) => None,
        }
    }

    /// Returns mutable popup policy when this node is transient.
    fn popup_mut(&mut self) -> Option<&mut PopupState> {
        match &mut self.kind {
            SurfaceKind::Popup(popup) => Some(popup),
            SurfaceKind::Root(_) => None,
        }
    }

    /// Returns whether manager chrome or application content owns pointer capture.
    fn has_capture(&self) -> bool {
        self.root().is_some_and(|root| root.interaction != RootInteraction::None)
            || self.root().and_then(|root| root.menu_bar.as_ref()).is_some_and(MenuSurface::has_capture)
            || self.surface.body.has_capture()
    }

    /// Clears every transient input identity retained by this surface.
    fn clear_transient_targets(&mut self) {
        // Only roots have manager chrome, while both roles own a concrete widget runtime.
        if let Some(root) = self.root_mut() {
            root.interaction = RootInteraction::None;
        }
        if let Some(root) = self.root_mut()
            && let Some(menu) = root.menu_bar.as_mut()
        {
            menu.clear_pointer_targets();
        }
        self.surface.body.clear_transient_targets();
    }

    /// Clears chrome and body pointer targets without erasing retained keyboard focus.
    fn clear_pointer_targets(&mut self) {
        if let Some(root) = self.root_mut() {
            root.interaction = RootInteraction::None;
        }
        if let Some(root) = self.root_mut()
            && let Some(menu) = root.menu_bar.as_mut()
        {
            menu.clear_pointer_targets();
        }
        self.surface.body.clear_pointer_targets();
    }

    /// Shows or hides a root without dropping its application state.
    fn set_root_visible(&mut self, visible: bool) {
        let root = self.root_mut().expect("visibility is defined only for root nodes");
        root.visible = visible;
        if !visible {
            self.clear_transient_targets();
        }
    }

    /// Replaces root chrome options and revokes a gesture disabled by the new policy.
    fn set_root_options(&mut self, options: WindowOption) {
        self.surface.options = options;
        let interaction = self.root().expect("chrome options are defined only for roots").interaction;
        let disabled = (options.intersects(WindowOption::NO_TITLE) && interaction == RootInteraction::Moving)
            || (options.intersects(WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE) && interaction == RootInteraction::Resizing);
        if disabled {
            self.clear_transient_targets();
        }
    }

    /// Returns the committed root-chrome region under one screen-space point.
    fn chrome_part_at(&self, point: Vec2i) -> Option<RootChromePart> {
        self.root()?;
        // Chrome outside an ancestor-provided child clip is neither visible nor interactive.
        self.surface.clip.contains(&point).then(|| self.surface.geometry.hit_test(point)).flatten()
    }

    /// Returns whether a parent-owned overlay precedes structural children at `point`.
    fn overlay_contains(&self, point: Vec2i) -> bool {
        // The frame border is visual chrome even though it has no action of its own. Selecting the
        // parent there acts as a hit shield, matching the border that is repainted after children.
        let frame = self.root().is_some()
            && self.surface.options.intersects(WindowOption::FRAME)
            && self.surface.clip.contains(&point)
            && self.surface.rect.contains(&point)
            && !self.surface.geometry.client.contains(&point);

        // Actionable title/close/resize geometry and the intrinsic menu bar use their committed
        // rectangles. Together with `frame`, these are precisely the parent-owned regions recorded
        // in the overlay pass rather than the earlier application-content pass.
        frame || self.chrome_part_at(point).is_some() || self.root().and_then(|root| root.menu_bar.as_ref()).is_some_and(|menu| menu.contains(point))
    }

    /// Emits one concrete window event at the next safe application dispatch boundary.
    fn emit_window_event(&mut self, event: WindowEvent) {
        // The strong port shares the window node's exact lifetime, while subscribers carry only a
        // weak capability and cannot emit into this queue themselves.
        self.root().expect("window events are emitted only by roots").events.borrow_mut().emit(event);
    }

    /// Dismisses an active popup and emits exactly one policy event.
    fn dismiss_popup(&mut self) {
        // The caller walks the active leaf upward, so descendants are always notified first.
        self.surface.body.clear_transient_targets();
        if let Some(PopupState::Application { events, .. }) = self.popup() {
            // Menu popups have no unobservable lifecycle port; their visibility is wholly internal.
            events.borrow_mut().emit(PopupEvent::Dismissed);
        }
    }

    /// Consumes the application-popup request to focus its first target after layout.
    fn take_popup_focus_request(&mut self) -> bool {
        let Some(PopupState::Application { focus_on_layout, .. }) = self.popup_mut() else {
            return false;
        };
        // A request belongs to one visibility transition and must not reset focus every frame.
        std::mem::take(focus_on_layout)
    }

    /// Lays out this node and its optional root-owned menu bar through concrete bodies.
    fn layout(&mut self, style: &Style, atlas: &crate::AtlasHandle, viewport: Recti) {
        let menu_bar = match &mut self.kind {
            SurfaceKind::Root(root) => root.menu_bar.as_mut(),
            SurfaceKind::Popup(_) => None,
        };
        self.surface.layout(menu_bar, style, atlas, viewport);
    }
}

/// Concrete ownership forest plus incrementally maintained traversal order.
///
/// `nodes` owns every application tree exactly once, and the relative order of its root nodes is the
/// global back-to-front activation chronology. Popup nodes may be interleaved without participating
/// in that chronology because their sole active branch is positioned from parent edges.
/// `active_popup` stores only the deepest visible popup; its visible ancestors are derived from the
/// parent edges. `visible_order` is the reusable layered, parent-first traversal consumed by layout,
/// retained updates, and diagnostics. Paint and hit testing recurse over the same child edges to
/// place each parent's overlays on the far side of its descendants.
pub(super) struct SurfaceForest {
    /// Retained surface storage and authoritative chronological order for root nodes.
    ///
    /// Indices are deliberately not exposed as identity because fronting a root moves its complete
    /// node to this vector's tail. Stable keys and parent edges remain valid across that move.
    nodes: Vec<SurfaceNode>,
    /// Deepest popup in the sole active branch, or no visible popup branch.
    active_popup: Option<PopupId>,
    /// Materialized parent-first base traversal, reused without per-frame allocation.
    ///
    /// Fixed and modal bands remain back-to-front globally. Within a structural family, however,
    /// this order describes layout/update ownership; paint and pointer selection additionally
    /// account for the parent overlay that follows all of its child surfaces.
    visible_order: Vec<SurfaceKey>,
    /// Reusable child-to-parent workspace used while materializing the active popup path.
    popup_path_scratch: Vec<SurfaceKey>,
}

impl SurfaceForest {
    /// Creates an empty concrete forest with reusable visible-order workspaces.
    pub(super) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            active_popup: None,
            visible_order: Vec::new(),
            popup_path_scratch: Vec::new(),
        }
    }

    /// Resolves a stable surface key to its current storage index.
    fn node_index(&self, key: SurfaceKey) -> Option<usize> {
        self.nodes.iter().position(|node| node.key == key)
    }

    /// Borrows one retained surface node by stable identity.
    fn node(&self, key: SurfaceKey) -> Option<&SurfaceNode> {
        self.node_index(key).map(|index| &self.nodes[index])
    }

    /// Mutably borrows one retained surface node by stable identity.
    fn node_mut(&mut self, key: SurfaceKey) -> Option<&mut SurfaceNode> {
        let index = self.node_index(key)?;
        Some(&mut self.nodes[index])
    }

    /// Borrows one retained root node.
    fn root_node(&self, root: RootId) -> Option<&SurfaceNode> {
        self.node(SurfaceKey::Root(root))
    }

    /// Mutably borrows one retained root node.
    fn root_node_mut(&mut self, root: RootId) -> Option<&mut SurfaceNode> {
        self.node_mut(SurfaceKey::Root(root))
    }

    /// Borrows one retained popup node.
    fn popup_node(&self, popup: PopupId) -> Option<&SurfaceNode> {
        self.node(SurfaceKey::Popup(popup))
    }

    /// Mutably borrows one retained popup node.
    fn popup_node_mut(&mut self, popup: PopupId) -> Option<&mut SurfaceNode> {
        self.node_mut(SurfaceKey::Popup(popup))
    }

    /// Borrows common surface storage for one traversal key.
    fn surface(&self, key: SurfaceKey) -> Option<&Surface> {
        self.node(key).map(|node| &node.surface)
    }

    /// Mutably borrows common surface storage for one traversal key.
    fn surface_mut(&mut self, key: SurfaceKey) -> Option<&mut Surface> {
        self.node_mut(key).map(|node| &mut node.surface)
    }

    /// Inserts a root at the newest point in global activation chronology.
    fn insert_root(&mut self, node: SurfaceNode) {
        debug_assert!(matches!(node.key, SurfaceKey::Root(_)) && node.root().is_some());
        // Registration is chronologically newest. Hidden dialogs do not participate until shown,
        // and showing them moves the same node to the tail again.
        self.nodes.push(node);
        self.rebuild_visible_order();
    }

    /// Inserts an inactive popup; visibility remains derived from `active_popup`.
    fn insert_popup(&mut self, node: SurfaceNode) {
        debug_assert!(matches!(node.key, SurfaceKey::Popup(_)) && node.popup().is_some());
        self.nodes.push(node);
    }

    /// Moves one complete root node to the newest point in global activation chronology.
    fn move_root_to_front(&mut self, root: RootId) {
        let index = self
            .node_index(SurfaceKey::Root(root))
            .filter(|index| self.nodes[*index].root().is_some())
            .expect("fronted root must remain retained");
        // Vec::remove performs only shallow Rust moves of the trailing records; widget allocations,
        // event ports, stable keys, and parent edges retain their identities.
        let node = self.nodes.remove(index);
        self.nodes.push(node);
        self.rebuild_visible_order();
    }

    /// Changes one normal root's fixed layer without changing its activation chronology.
    fn set_root_layer(&mut self, root: RootId, layer: u8) {
        let state = self
            .root_node_mut(root)
            .and_then(SurfaceNode::root_mut)
            .expect("layer mutation requires a retained root");
        // The node stays at its existing chronological position; layered traversal filters that
        // single order when rebuilding the visible cache.
        state.mode = RootMode::Normal { layer };
        self.rebuild_visible_order();
    }

    /// Resolves the direct root parent stored for a child window or modal dialog.
    fn root_parent(&self, root: RootId) -> Option<RootId> {
        // Popup parents are impossible for root nodes; retain the explicit match so a malformed
        // forest cannot silently turn a non-root edge into a structural window relationship.
        match self.root_node(root)?.parent {
            Some(SurfaceKey::Root(parent)) => Some(parent),
            Some(SurfaceKey::Popup(_)) | None => None,
        }
    }

    /// Resolves the global stacking band inherited by one root.
    fn stacking_mode(&self, mut root: RootId) -> Option<RootMode> {
        // Child construction only points toward an already retained ancestor, so this loop cannot
        // cycle. Following values rather than retaining a second layer mirror keeps parent changes
        // immediately authoritative for every descendant.
        loop {
            let node = self.root_node(root)?;
            match node.root()?.mode {
                mode @ (RootMode::Normal { .. } | RootMode::Modal) => return Some(mode),
                RootMode::Child => root = self.root_parent(root)?,
            }
        }
    }

    /// Returns whether one root and every structural owner currently request visibility.
    fn root_is_effectively_visible(&self, root: RootId) -> bool {
        // Unknown/non-root keys are conservatively absent. A separate local bit is necessary so a
        // hidden ancestor suppresses participation without permanently hiding each child record.
        let Some(node) = self.root_node(root) else {
            return false;
        };
        let Some(state) = node.root() else {
            return false;
        };
        if !state.visible {
            return false;
        }
        match state.mode {
            RootMode::Normal { .. } => true,
            RootMode::Child | RootMode::Modal => self.root_parent(root).is_some_and(|parent| self.root_is_effectively_visible(parent)),
        }
    }

    /// Selects the adjacent visible ordinary root in activation chronology, with wrapping.
    fn window_cycle_target(&self, current: Option<RootId>, reverse: bool) -> Option<RootId> {
        // A keyboard window switch is rare, so materializing only the compact root identities keeps
        // the wrap and direction policy obvious. Forest chronology remains authoritative even for
        // structural children whose visible traversal is nested beneath a parent.
        let roots: Vec<RootId> = self
            .nodes
            .iter()
            .filter_map(|node| match (node.key, node.root()) {
                (SurfaceKey::Root(root), Some(state))
                    if matches!(state.mode, RootMode::Normal { .. } | RootMode::Child) && self.root_is_effectively_visible(root) =>
                {
                    Some(root)
                }
                _ => None,
            })
            .collect();
        let count = roots.len();
        if count == 0 {
            return None;
        }

        // A manager without an explicit active root starts at the appropriate chronological edge.
        // Normally `current` is the front keyboard surface, so this fallback matters only while no
        // eligible surface currently owns keyboard routing.
        let Some(index) = current.and_then(|current| roots.iter().position(|root| *root == current)) else {
            return Some(if reverse { roots[count - 1] } else { roots[0] });
        };
        let target = if reverse {
            index.checked_sub(1).unwrap_or(count - 1)
        } else {
            (index + 1) % count
        };
        Some(roots[target])
    }

    /// Appends one visible child-window family in parent-first base traversal order.
    fn append_visible_root_tree(&self, root: RootId, output: &mut Vec<SurfaceKey>) {
        // The caller admits only an effectively visible root. Descending through direct Child
        // edges preserves forest chronology among siblings while keeping unrelated descendants
        // out of this family's contiguous layout/update traversal.
        output.push(SurfaceKey::Root(root));
        for node in &self.nodes {
            let (SurfaceKey::Root(child), Some(state)) = (node.key, node.root()) else {
                continue;
            };
            if state.mode == RootMode::Child && node.parent == Some(SurfaceKey::Root(root)) && state.visible {
                self.append_visible_root_tree(child, output);
            }
        }
    }

    /// Returns the newest effectively visible modal root from global activation chronology.
    fn active_modal_root(&self) -> Option<RootId> {
        // Reverse chronology selects exactly the dialog painted last in the dedicated modal band.
        // Effective visibility prevents a dialog below a hidden structural owner from resurfacing.
        self.nodes.iter().rev().find_map(|node| match (node.key, node.root()) {
            (SurfaceKey::Root(root), Some(state)) if state.mode == RootMode::Modal && self.root_is_effectively_visible(root) => Some(root),
            _ => None,
        })
    }

    /// Follows sole parent edges to the root that owns one surface.
    fn owning_root(&self, mut key: SurfaceKey) -> Option<RootId> {
        loop {
            match key {
                SurfaceKey::Root(root) => return self.root_node(root).map(|_| root),
                SurfaceKey::Popup(_) => key = self.node(key)?.parent?,
            }
        }
    }

    /// Returns a popup's zero-based depth below its owning root.
    fn popup_depth(&self, popup: PopupId) -> Option<usize> {
        let mut depth = 0usize;
        let mut key = SurfaceKey::Popup(popup);
        loop {
            match self.node(key)?.parent? {
                SurfaceKey::Root(_) => return Some(depth),
                parent @ SurfaceKey::Popup(_) => {
                    depth = depth.saturating_add(1);
                    key = parent;
                }
            }
        }
    }

    /// Returns whether one popup is an ancestor-or-self of the active leaf.
    fn popup_is_active(&self, popup: PopupId) -> bool {
        let mut current = self.active_popup;
        while let Some(candidate) = current {
            if candidate == popup {
                return true;
            }
            current = match self.popup_node(candidate).and_then(|node| node.parent) {
                Some(SurfaceKey::Popup(parent)) => Some(parent),
                _ => None,
            };
        }
        false
    }

    /// Returns the first popup below the root in the active branch.
    fn active_top_popup(&self) -> Option<PopupId> {
        let mut current = self.active_popup?;
        while let Some(SurfaceKey::Popup(parent)) = self.popup_node(current).and_then(|node| node.parent) {
            current = parent;
        }
        Some(current)
    }

    /// Replaces the deepest authoritative popup and refreshes derived visibility.
    fn set_active_popup(&mut self, popup: Option<PopupId>) {
        self.active_popup = popup;
        self.rebuild_visible_order();
    }

    /// Materializes the active branch parent-first into caller-owned reusable storage.
    fn fill_active_popup_path(&self, output: &mut Vec<SurfaceKey>) {
        output.clear();
        let mut current = self.active_popup;
        while let Some(popup) = current {
            output.push(SurfaceKey::Popup(popup));
            current = match self.popup_node(popup).and_then(|node| node.parent) {
                Some(SurfaceKey::Popup(parent)) => Some(parent),
                _ => None,
            };
        }
        output.reverse();
    }

    /// Rebuilds layered visibility by filtering the one chronological node order.
    fn rebuild_visible_order(&mut self) {
        let active_owner = self.active_popup.and_then(|popup| self.owning_root(SurfaceKey::Popup(popup)));
        let active_mode = active_owner.and_then(|root| self.stacking_mode(root));

        let mut popup_path = std::mem::take(&mut self.popup_path_scratch);
        self.fill_active_popup_path(&mut popup_path);
        let mut visible = std::mem::take(&mut self.visible_order);
        visible.clear();

        // Sixteen fixed scans admit only parentless family roots. Recursive insertion then keeps
        // each child family contiguous while preserving chronology independently among top-level
        // peers and among children that share one direct parent.
        for layer in MIN_LAYER..=MAX_LAYER {
            for node in &self.nodes {
                if let (SurfaceKey::Root(root), Some(state)) = (node.key, node.root())
                    && state.mode == (RootMode::Normal { layer })
                    && self.root_is_effectively_visible(root)
                {
                    self.append_visible_root_tree(root, &mut visible);
                }
            }
            if active_mode == Some(RootMode::Normal { layer }) {
                visible.extend_from_slice(&popup_path);
            }
        }
        // Modal roots remain a manager-owned tier above every fixed family. They are not structural
        // children for composition even though their sole parent edge still controls ownership.
        for node in &self.nodes {
            if let (SurfaceKey::Root(root), Some(state)) = (node.key, node.root())
                && state.mode == RootMode::Modal
                && self.root_is_effectively_visible(root)
            {
                visible.push(SurfaceKey::Root(root));
            }
        }
        if active_mode == Some(RootMode::Modal) {
            visible.extend_from_slice(&popup_path);
        }

        self.visible_order = visible;
        self.popup_path_scratch = popup_path;
    }

    /// Removes roots and all popup descendants whose derived owner is in `roots`.
    fn remove_roots(&mut self, roots: &[RootId]) {
        let removed = self
            .nodes
            .iter()
            .filter_map(|node| self.owning_root(node.key).filter(|owner| roots.contains(owner)).map(|_| node.key))
            .collect::<Vec<_>>();
        // Removing complete nodes also removes them from the authoritative chronological order, so
        // no secondary stacking structure needs reconciliation.
        self.nodes.retain(|node| !removed.contains(&node.key));
        if self.active_popup.is_some_and(|popup| removed.contains(&SurfaceKey::Popup(popup))) {
            self.active_popup = None;
        }
        self.rebuild_visible_order();
    }
}

impl WindowManager {
    /// Borrows one mounted menu item's concrete public state through its stable typed capability.
    pub(crate) fn menu_item(&self, handle: &crate::MenuItemHandle) -> Result<&crate::MenuItemParameters, crate::MenuItemAccessError> {
        // Search authoritative bar and popup records by their process-unique item IDs. Forest
        // membership rejects unmounted and foreign handles without consulting event allocations or
        // maintaining an application-visible lookup table.
        for node in &self.surfaces.nodes {
            if let Some(item) = node.root().and_then(|root| root.menu_bar.as_ref()).and_then(|menu| menu.item(handle)) {
                return Ok(&item.parameters);
            }
            if let Some(item) = node.surface.body.menu().and_then(|menu| menu.item(handle)) {
                return Ok(&item.parameters);
            }
        }
        Err(crate::MenuItemAccessError::UnknownItem)
    }

    /// Mutably borrows one mounted menu item's concrete public state through its stable capability.
    pub(crate) fn menu_item_mut(&mut self, handle: &crate::MenuItemHandle) -> Result<&mut crate::MenuItemParameters, crate::MenuItemAccessError> {
        // Validate before changing transaction state so an unmounted or foreign handle is a true
        // no-op. The second linear scan is intentional; menu collections remain very small.
        self.menu_item(handle)?;
        self.invalidate_ui_commit();
        for node in &mut self.surfaces.nodes {
            match (&mut node.kind, &mut node.surface.body) {
                (SurfaceKind::Root(root), _) => {
                    if let Some(item) = root.menu_bar.as_mut().and_then(|menu| menu.item_mut(handle)) {
                        return Ok(&mut item.parameters);
                    }
                }
                (SurfaceKind::Popup(PopupState::Menu { .. }), SurfaceBody::Menu(menu)) => {
                    if let Some(item) = menu.item_mut(handle) {
                        return Ok(&mut item.parameters);
                    }
                }
                (SurfaceKind::Popup(_), SurfaceBody::Widgets { .. }) => {}
                (SurfaceKind::Popup(_), SurfaceBody::Menu(_)) => {
                    unreachable!("surface role and concrete body must remain structurally aligned")
                }
            }
        }
        unreachable!("a validated menu item must remain mounted during one exclusive manager borrow")
    }

    /// Registers one concrete root after validating the parent contract implied by its role.
    fn register_window(
        &mut self,
        mode: RootMode,
        parent: Option<RootId>,
        window: Window,
        options: WindowOption,
        visible: bool,
    ) -> Result<WindowHandle, SurfaceMutationError> {
        match mode {
            RootMode::Normal { .. } => {
                debug_assert!(parent.is_none(), "independent roots cannot have a structural parent");
            }
            RootMode::Child => {
                let owner = parent.ok_or(SurfaceMutationError::InvalidChildWindowParent)?;
                let owner = self.root_node(owner)?.root().expect("root lookup must return root policy");
                if !matches!(owner.mode, RootMode::Normal { .. } | RootMode::Child) {
                    return Err(SurfaceMutationError::InvalidChildWindowParent);
                }
            }
            RootMode::Modal => {
                let owner = parent.ok_or(SurfaceMutationError::InvalidDialogOwner)?;
                let owner = self.root_node(owner)?.root().expect("root lookup must return root policy");
                if !matches!(owner.mode, RootMode::Normal { .. } | RootMode::Child) {
                    return Err(SurfaceMutationError::InvalidDialogOwner);
                }
            }
        }

        let (name, rect, content, menu_bar, child_window_clip) = window.into_parts();
        // Move top-level labels into the bar while retaining only their row vectors until the root
        // exists. This flat transfer avoids constructing a recursive hierarchy beside the forest.
        let mut menu_popups = Vec::new();
        let menu_bar = menu_bar.map(|bar| {
            let menus = bar.into_menus();
            let mut headings = Vec::with_capacity(menus.len());
            menu_popups.reserve(menus.len());
            for menu in menus {
                let (label, entries) = menu.into_parts();
                headings.push(MenuSlot::Branch { label });
                menu_popups.push(entries);
            }
            MenuSurface::new(headings, false)
        });

        // Identity and event delivery are allocated separately, then paired once in the returned
        // application handle. The forest owns the same ID and the only strong event endpoint.
        let id = RootId::allocate();
        let events = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let event_handle = crate::WidgetEventPortHandle::new(&events);
        self.surfaces.insert_root(SurfaceNode {
            key: SurfaceKey::Root(id),
            parent: parent.map(SurfaceKey::Root),
            surface: Surface::new(name, options, rect, content),
            kind: SurfaceKind::Root(RootState {
                mode,
                visible,
                interaction: RootInteraction::None,
                events,
                menu_bar,
                child_window_clip,
            }),
        });

        // Each row vector becomes a direct child edge; recursion consumes only one parent's pending
        // children at a time, so no second menu tree coexists with the authoritative forest.
        for (trigger_slot, entries) in menu_popups.into_iter().enumerate() {
            self.register_menu_popup(SurfaceKey::Root(id), trigger_slot, entries);
        }
        self.surfaces.rebuild_visible_order();
        self.invalidate_ui_commit();
        Ok(WindowHandle::new(id, event_handle))
    }

    /// Creates an open independent window from one complete retained definition.
    pub fn create_window(&mut self, window: Window) -> WindowHandle {
        // Independent construction has no fallible owner edge.
        self.register_window(RootMode::Normal { layer: DEFAULT_LAYER }, None, window, WindowOption::FRAME, true)
            .expect("ordinary window registration cannot fail")
    }

    /// Creates one visible ordinary window structurally owned by `parent`.
    pub fn create_child_window(&mut self, parent: &WindowHandle, window: Window) -> Result<WindowHandle, SurfaceMutationError> {
        // Authenticate and classify the parent before consuming the uniquely owned Window. Child
        // registration then adds one backward edge, which makes cycles unrepresentable without a
        // later reparenting operation (none is exposed).
        let parent = self.window_id(parent)?;
        self.register_window(RootMode::Child, Some(parent), window, WindowOption::FRAME, true)
    }

    /// Creates a hidden modal dialog directly owned by an independent or structural child window.
    pub fn create_dialog(&mut self, owner: &WindowHandle, window: Window) -> Result<WindowHandle, SurfaceMutationError> {
        // Authenticate before consuming or mounting the supplied dialog definition.
        let owner = self.window_id(owner)?;
        self.register_window(RootMode::Modal, Some(owner), window, WindowOption::FRAME, false)
    }

    /// Creates a hidden top-level popup definition inside one window or dialog.
    pub fn create_popup(&mut self, owner: &WindowHandle, name: &str, content: Node) -> Result<PopupHandle, SurfaceMutationError> {
        // Resolve the authenticated window capability before transferring the content node.
        let owner = self.window_id(owner)?;
        // A top-level popup's sole parent edge points directly to its owning root.
        Ok(self.register_application_popup(SurfaceKey::Root(owner), name, content))
    }

    /// Registers one application-authored popup below an already validated root.
    fn register_application_popup(&mut self, parent: SurfaceKey, name: &str, content: Node) -> PopupHandle {
        let id = PopupId::allocate();
        // The forest node retains the strong lifecycle port while the application handle combines
        // an independent stable ID with a weak endpoint for the same registered popup.
        let events = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let event_handle = crate::WidgetEventPortHandle::new(&events);
        self.surfaces.insert_popup(SurfaceNode {
            key: SurfaceKey::Popup(id),
            parent: Some(parent),
            surface: Surface::new(name.to_owned(), Self::default_popup_options(), Recti::default(), content),
            kind: SurfaceKind::Popup(PopupState::Application {
                anchor: Recti::default(),
                events,
                focus_on_layout: false,
            }),
        });
        self.invalidate_ui_commit();
        PopupHandle::new(id, event_handle)
    }

    /// Consumes one declaration row vector directly into a concrete menu-popup forest node.
    fn register_menu_popup(&mut self, parent: SurfaceKey, trigger_slot: usize, entries: Vec<MenuEntry>) {
        let mut rows = Vec::with_capacity(entries.len());
        let mut children = Vec::new();
        for (slot, entry) in entries.into_iter().enumerate() {
            match entry {
                MenuEntry::Item(item) => rows.push(MenuSlot::Item(item.record)),
                MenuEntry::Separator => rows.push(MenuSlot::Separator),
                MenuEntry::Submenu(menu) => {
                    // A branch label stays in this parent surface; only its owned row vector waits
                    // until the parent forest node exists and can become the child's sole edge.
                    let (label, entries) = menu.into_parts();
                    rows.push(MenuSlot::Branch { label });
                    children.push((slot, entries));
                }
            }
        }
        let id = PopupId::allocate();
        self.surfaces.insert_popup(SurfaceNode {
            key: SurfaceKey::Popup(id),
            parent: Some(parent),
            surface: Surface::menu(MenuSurface::new(rows, true)),
            kind: SurfaceKind::Popup(PopupState::Menu { trigger_slot }),
        });
        // The parent is retained before recursion, satisfying the forest's sole structural invariant.
        for (trigger_slot, entries) in children {
            self.register_menu_popup(SurfaceKey::Popup(id), trigger_slot, entries);
        }
    }

    /// Replaces a retained window title after authenticating its application capability.
    pub fn set_window_name(&mut self, window: &WindowHandle, name: String) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        self.root_node_mut(root)?.surface.name = name;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a window outer rectangle silently.
    pub fn set_window_rect(&mut self, window: &WindowHandle, rect: Recti) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        self.root_node_mut(root)?.surface.rect = rect;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a window outer size without changing its screen origin.
    pub fn set_window_size(&mut self, window: &WindowHandle, size: Dimensioni) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        let surface = &mut self.root_node_mut(root)?.surface;
        surface.rect.width = size.width;
        surface.rect.height = size.height;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces window chrome options and reconciles capture immediately.
    pub fn set_window_options(&mut self, window: &WindowHandle, options: WindowOption) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        self.root_node_mut(root)?.set_root_options(options);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces popup frame, padding, and automatic-size policy through its typed handle.
    pub fn set_popup_options(&mut self, popup: &PopupHandle, options: WindowOption) -> Result<(), SurfaceMutationError> {
        let node = self.popup_node_mut(popup)?;
        // Popups never acquire manager chrome; enforce that invariant regardless of caller flags.
        node.surface.options = options | WindowOption::NO_TITLE | WindowOption::NO_RESIZE;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Assigns an independent window to one fixed application stacking layer.
    pub fn set_window_layer(&mut self, window: &WindowHandle, layer: u8) -> Result<(), SurfaceMutationError> {
        // Validate the public numeric domain before resolving structural policy, preserving the
        // established error precedence for every kind of authenticated window handle.
        if layer > MAX_LAYER {
            return Err(SurfaceMutationError::InvalidLayer(layer));
        }
        let root = self.window_id(window)?;
        let state = self.root_node(root)?.root().expect("root lookup must return root policy");
        // Child bands follow their family root and dialogs occupy the modal band, so neither may
        // acquire a conflicting independent layer value.
        if !matches!(state.mode, RootMode::Normal { .. }) {
            return Err(SurfaceMutationError::ManagedLayer);
        }
        self.surfaces.set_root_layer(root, layer);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Returns the effective fixed or modal layer policy of one window.
    pub fn window_layer(&self, window: &WindowHandle) -> Result<LayerBinding, SurfaceMutationError> {
        let root = self.window_id(window)?;
        // Resolve through Child edges at read time so changing a family root is immediately visible
        // to every descendant without synchronizing cached layer mirrors.
        let mode = self.surfaces.stacking_mode(root).ok_or(SurfaceMutationError::UnknownWindow)?;
        Ok(match mode {
            RootMode::Normal { layer } => LayerBinding::Fixed(layer),
            RootMode::Modal => LayerBinding::Modal,
            RootMode::Child => unreachable!("stacking_mode resolves every child to an ancestor band"),
        })
    }

    /// Shows or hides a window while retaining its application and popup definitions.
    pub fn set_window_visible(&mut self, window: &WindowHandle, visible: bool) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        self.set_window_visible_id(root, visible)
    }

    /// Applies visibility policy to one already authenticated internal window identity.
    fn set_window_visible_id(&mut self, root: RootId, visible: bool) -> Result<(), SurfaceMutationError> {
        let mode = self.root_node(root)?.root().expect("root lookup must return root policy").mode;
        if visible {
            // Dialogs additionally require an effectively visible ordinary owner. Ordinary windows
            // may retain a true local bit beneath a hidden parent and recover with that parent.
            if mode == RootMode::Modal {
                let owner = match self.root_node(root)?.parent {
                    Some(SurfaceKey::Root(owner)) => owner,
                    _ => return Err(SurfaceMutationError::InvalidDialogOwner),
                };
                if !self.root_is_visible(owner) {
                    return Err(SurfaceMutationError::InvalidDialogOwner);
                }
                if self.keyboard_menu_root.is_some_and(|menu_root| menu_root != root) {
                    // A newly active modal owns the only keyboard scope; an underlying menu cannot
                    // remain highlighted or consume keys through that boundary.
                    self.finish_keyboard_menu(true);
                }
                if self.surfaces.active_modal_root() != Some(root) {
                    self.dismiss_active_popups();
                }
            }
            self.root_node_mut(root)?.set_root_visible(true);
            self.surfaces.move_root_to_front(root);
            if mode == RootMode::Modal {
                // A newly visible dialog is the concrete keyboard surface for its modal group.
                self.active_surface = Some(SurfaceKey::Root(root));
            }
        } else {
            // Descendant children retain their own visibility intent, but every modal descendant is
            // explicitly closed so showing the family again cannot resurrect a dismissed dialog.
            let affected = self.owned_root_ids(root);
            if self.keyboard_menu_root.is_some_and(|menu_root| affected.contains(&menu_root)) {
                self.finish_keyboard_menu(true);
            }
            if self.active_popup_owner().is_some_and(|owner| affected.contains(&owner)) {
                self.dismiss_active_popups();
            }
            self.root_node_mut(root)?.set_root_visible(false);
            for affected_root in affected.iter().copied().filter(|affected| *affected != root) {
                let node = self.root_node_mut(affected_root).expect("collected root must remain retained");
                if node.root().is_some_and(|state| state.mode == RootMode::Modal) {
                    node.set_root_visible(false);
                } else {
                    // Hidden structural descendants are absent from traversal, so revoke their
                    // transient identities without overwriting the visibility they should recover.
                    node.clear_transient_targets();
                }
            }
            if self
                .active_surface
                .and_then(|active| self.surfaces.owning_root(active))
                .is_some_and(|active| affected.contains(&active))
            {
                // Structural children and dialogs return to their direct owning root. Independent
                // windows have no parent and leave front-surface fallback to choose the next root.
                self.active_surface = self.surfaces.root_node(root).and_then(|node| node.parent);
            }
            self.surfaces.rebuild_visible_order();
            self.reconcile_active_surface();
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Shows one popup at the current pointer position.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), SurfaceMutationError> {
        let mouse = self.input.snapshot().mouse_pos;
        self.show_popup_at(popup, rect(mouse.x, mouse.y, 1, 1))
    }

    /// Shows one popup at an exact screen-space anchor and updates the sole active path.
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), SurfaceMutationError> {
        // Resolve the stable ID before opening; foreign and destroyed handles have no matching
        // application-popup node in this forest.
        let popup = self.popup_id(popup)?;
        // Open first so an invalid child path cannot partially replace its retained anchor.
        self.open_popup_id(popup)?;
        if self.keyboard_menu_root.is_some() {
            // Successful application-popup replacement already dismissed the menu ancestry. Clear
            // only its keyboard selection so the newly opened popup remains active.
            self.finish_keyboard_menu(false);
        }
        let node = self.surfaces.popup_node_mut(popup).expect("authenticated popup must remain retained");
        let PopupState::Application { anchor: current, focus_on_layout, .. } = node.popup_mut().expect("popup lookup must return popup policy") else {
            return Err(SurfaceMutationError::UnknownPopup);
        };
        *current = anchor;
        // Geometry is committed later in this transaction, so defer first-focus selection until
        // the popup tree has valid allocations and clipping.
        *focus_on_layout = true;
        node.surface.rect = anchor;
        self.active_surface = Some(SurfaceKey::Popup(popup));
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Opens one retained popup identity without replacing its configured placement relation.
    fn open_popup_id(&mut self, popup: PopupId) -> Result<(), SurfaceMutationError> {
        let node = self.surfaces.popup_node(popup).ok_or(SurfaceMutationError::UnknownPopup)?;
        let parent = node.parent.expect("every popup must retain one parent edge");
        let owner = self.surfaces.owning_root(SurfaceKey::Popup(popup)).ok_or(SurfaceMutationError::UnknownPopup)?;
        if !self.popup_owner_is_eligible(owner) {
            return Err(SurfaceMutationError::InvalidPopupParent);
        }

        // A top-level popup may replace another branch. A child is legal only while its sole parent
        // edge is in the active ancestry; no separately stored owner/path pair can disagree.
        let (keep, retained) = match parent {
            SurfaceKey::Root(_) => {
                let retained = self.surfaces.popup_is_active(popup);
                (usize::from(retained), retained)
            }
            SurfaceKey::Popup(parent) => {
                if !self.surfaces.popup_is_active(parent) {
                    return Err(SurfaceMutationError::InvalidPopupParent);
                }
                let parent_depth = self.surfaces.popup_depth(parent).expect("active popup must have a rooted ancestry");
                let retained = self.surfaces.popup_is_active(popup);
                (parent_depth + 1 + usize::from(retained), retained)
            }
        };

        self.truncate_active_popup_path(keep);
        if !retained {
            self.surfaces.set_active_popup(Some(popup));
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Hides one active popup and all of its active descendants.
    pub fn hide_popup(&mut self, popup: &PopupHandle) -> Result<(), SurfaceMutationError> {
        let popup = self.popup_id(popup)?;
        if self.surfaces.popup_is_active(popup) {
            let depth = self.surfaces.popup_depth(popup).expect("active popup must have rooted ancestry");
            self.truncate_active_popup_path(depth);
        }
        Ok(())
    }

    /// Raises one authenticated window inside its structural layer.
    pub fn bring_window_to_front(&mut self, window: &WindowHandle) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        self.bring_window_to_front_id(root)
    }

    /// Raises one already authenticated internal window identity inside its structural layer.
    fn bring_window_to_front_id(&mut self, root: RootId) -> Result<(), SurfaceMutationError> {
        let state = self.root_node(root)?.root().expect("root lookup must return root policy");
        let mode = state.mode;
        let visible = state.visible;
        if mode == RootMode::Modal && visible && self.surfaces.active_modal_root() != Some(root) {
            if self.keyboard_menu_root.is_some_and(|menu_root| menu_root != root) {
                self.finish_keyboard_menu(true);
            }
            self.dismiss_active_popups();
        }
        self.surfaces.move_root_to_front(root);
        if mode == RootMode::Modal && visible {
            // Fronting a visible dialog selects that concrete root as the modal keyboard surface.
            self.active_surface = Some(SurfaceKey::Root(root));
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Destroys one window together with every child window, dialog, and popup below it.
    pub fn destroy_window(&mut self, window: &WindowHandle) -> Result<(), SurfaceMutationError> {
        let root = self.window_id(window)?;
        // Materialize the root subtree before mutating storage so popup dismissal and active-window
        // cleanup can use one stable membership set.
        let removed = self.owned_root_ids(root);
        if self.keyboard_menu_root.is_some_and(|menu_root| removed.contains(&menu_root)) {
            self.finish_keyboard_menu(true);
        }
        if self.active_popup_owner().is_some_and(|owner| removed.contains(&owner)) {
            self.dismiss_active_popups();
        }
        if self
            .active_surface
            .and_then(|active| self.surfaces.owning_root(active))
            .is_some_and(|active| removed.contains(&active))
        {
            self.active_surface = self.surfaces.root_node(root).and_then(|node| node.parent);
        }
        self.surfaces.remove_roots(&removed);
        self.reconcile_active_surface();
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Resolves an already authenticated internal root identity to its concrete forest node.
    fn root_node(&self, root: RootId) -> Result<&SurfaceNode, SurfaceMutationError> {
        self.surfaces.root_node(root).ok_or(SurfaceMutationError::UnknownWindow)
    }

    /// Mutably resolves an already authenticated root identity to its concrete forest node.
    fn root_node_mut(&mut self, root: RootId) -> Result<&mut SurfaceNode, SurfaceMutationError> {
        self.surfaces.root_node_mut(root).ok_or(SurfaceMutationError::UnknownWindow)
    }

    /// Resolves one application-facing window handle to its process-unique concrete identity.
    fn window_id(&self, window: &WindowHandle) -> Result<RootId, SurfaceMutationError> {
        // Process-wide uniqueness makes membership in this forest sufficient authentication. A
        // stale or foreign handle cannot share its ID with any current node.
        self.surfaces
            .root_node(window.id())
            .and_then(SurfaceNode::root)
            .map(|_| window.id())
            .ok_or(SurfaceMutationError::UnknownWindow)
    }

    /// Resolves one application-facing popup handle to its process-unique concrete identity.
    fn popup_id(&self, popup: &PopupHandle) -> Result<PopupId, SurfaceMutationError> {
        // A menu popup can share the `PopupId` wrapper but never the same process-wide value. The
        // role check additionally preserves the public API boundary around application popups.
        self.surfaces
            .popup_node(popup.id())
            .and_then(SurfaceNode::popup)
            .filter(|popup| matches!(popup, PopupState::Application { .. }))
            .map(|_| popup.id())
            .ok_or(SurfaceMutationError::UnknownPopup)
    }

    /// Resolves a typed popup capability directly to its authenticated concrete forest node.
    #[cfg(test)]
    fn popup_node(&self, popup: &PopupHandle) -> Result<&SurfaceNode, SurfaceMutationError> {
        let id = self.popup_id(popup)?;
        Ok(self.surfaces.popup_node(id).expect("authenticated popup must remain retained"))
    }

    /// Mutably resolves a typed popup capability to its authenticated concrete forest node.
    fn popup_node_mut(&mut self, popup: &PopupHandle) -> Result<&mut SurfaceNode, SurfaceMutationError> {
        let id = self.popup_id(popup)?;
        Ok(self.surfaces.popup_node_mut(id).expect("authenticated popup must remain retained"))
    }

    /// Collects one root and every structurally owned child window or modal dialog below it.
    fn owned_root_ids(&self, root: RootId) -> Vec<RootId> {
        let mut ids = vec![root];
        // Parent edges always point toward an earlier retained root, so repeatedly scanning for
        // direct members reaches a fixed point without cycle detection or a second child registry.
        let mut index = 0;
        while index < ids.len() {
            let owner = ids[index];
            for node in &self.surfaces.nodes {
                let SurfaceKey::Root(candidate) = node.key else { continue };
                if node.parent == Some(SurfaceKey::Root(owner)) && node.root().is_some() && !ids.contains(&candidate) {
                    ids.push(candidate);
                }
            }
            index += 1;
        }
        ids
    }

    /// Returns whether a popup owner may participate under current visibility and modal policy.
    fn popup_owner_is_eligible(&self, owner: RootId) -> bool {
        let Some(state) = self.surfaces.root_node(owner).and_then(SurfaceNode::root) else {
            return false;
        };
        if !self.surfaces.root_is_effectively_visible(owner) {
            return false;
        }
        match self.surfaces.active_modal_root() {
            Some(modal) => owner == modal,
            None => matches!(state.mode, RootMode::Normal { .. } | RootMode::Child),
        }
    }

    /// Returns the owning root derived from the deepest active popup's ancestry.
    fn active_popup_owner(&self) -> Option<RootId> {
        self.surfaces.active_popup.and_then(|popup| self.surfaces.owning_root(SurfaceKey::Popup(popup)))
    }

    /// Removes the active path suffix after `keep`, notifying deepest popups first.
    fn truncate_active_popup_path(&mut self, keep: usize) {
        let Some(mut current) = self.surfaces.active_popup else { return };
        let restore_surface = match self.active_surface {
            Some(SurfaceKey::Popup(active)) if self.surfaces.popup_depth(active).is_some_and(|depth| depth >= keep) => {
                // The direct parent is the deterministic focus destination when the active popup
                // leaves the visible branch. It is either the retained popup prefix or its root.
                self.surfaces.popup_node(active).and_then(|node| node.parent)
            }
            _ => None,
        };
        let mut revoked_capture = false;
        let mut retained_leaf = Some(current);
        while self.surfaces.popup_depth(current).is_some_and(|depth| depth >= keep) {
            let parent = match self.surfaces.popup_node(current).and_then(|node| node.parent) {
                Some(SurfaceKey::Popup(parent)) => Some(parent),
                _ => None,
            };
            let popup = self.surfaces.popup_node_mut(current).expect("active popup must remain retained");
            // Once the popup leaves the active ancestry only the manager can consume the remainder
            // of an application capture gesture, so remember capture before clearing the runtime.
            revoked_capture |= popup.surface.body.has_capture();
            popup.dismiss_popup();
            retained_leaf = parent;
            let Some(parent) = parent else { break };
            current = parent;
        }
        self.discard_pointer_capture_tail |= revoked_capture;
        self.surfaces.set_active_popup(retained_leaf);
        if let Some(restore_surface) = restore_surface {
            self.active_surface = Some(restore_surface);
        }
        self.invalidate_ui_commit();
    }

    /// Dismisses the complete active popup branch.
    fn dismiss_active_popups(&mut self) {
        self.truncate_active_popup_path(0);
    }

    /// Returns whether one pointer event belongs to a capture revoked with a dismissed popup.
    fn discard_revoked_capture_event(&mut self, event: &UiInputEvent) -> bool {
        // A fresh press always starts a new gesture and cancels any stale suppression. Drag remains
        // swallowed while armed; the terminal release is swallowed once and clears the boundary.
        match event {
            UiInputEvent::MouseDown { .. } => {
                self.discard_pointer_capture_tail = false;
                false
            }
            UiInputEvent::MouseDrag { .. } => self.discard_pointer_capture_tail,
            UiInputEvent::MouseUp { .. } if self.discard_pointer_capture_tail => {
                self.discard_pointer_capture_tail = false;
                true
            }
            _ => false,
        }
    }

    /// Borrows the concrete menu presentation attached to one root or menu-popup key.
    fn menu_surface(&self, key: SurfaceKey) -> Option<&MenuSurface> {
        match key {
            SurfaceKey::Root(root) => self.surfaces.root_node(root)?.root()?.menu_bar.as_ref(),
            SurfaceKey::Popup(popup) => self.surfaces.popup_node(popup)?.surface.body.menu(),
        }
    }

    /// Mutably borrows the concrete menu presentation attached to one forest key.
    fn menu_surface_mut(&mut self, key: SurfaceKey) -> Option<&mut MenuSurface> {
        match key {
            SurfaceKey::Root(root) => self.surfaces.root_node_mut(root)?.root_mut()?.menu_bar.as_mut(),
            SurfaceKey::Popup(popup) => self.surfaces.popup_node_mut(popup)?.surface.body.menu_mut(),
        }
    }

    /// Returns whether a traversal surface belongs to the current input group.
    fn surface_is_eligible(&self, key: SurfaceKey, modal: Option<RootId>) -> bool {
        let Some(owner) = self.surfaces.owning_root(key) else {
            return false;
        };
        match modal {
            Some(modal) => owner == modal,
            None => self
                .surfaces
                .root_node(owner)
                .and_then(SurfaceNode::root)
                .is_some_and(|root| matches!(root.mode, RootMode::Normal { .. } | RootMode::Child)),
        }
    }

    /// Repairs active-surface identity after visibility or ownership changes.
    fn reconcile_active_surface(&mut self) {
        let modal = self.surfaces.active_modal_root();
        let retained = self
            .active_surface
            .is_some_and(|surface| self.surfaces.visible_order.contains(&surface) && self.surface_is_eligible(surface, modal));
        if !retained {
            // A modal root is the only mandatory fallback. Ordinary roots continue using the front
            // visible surface lazily until a pointer or window-cycle command selects one exactly.
            self.active_surface = modal.map(SurfaceKey::Root);
        }
    }

    /// Rewrites menu trigger presentation from the deepest authoritative popup leaf.
    fn sync_menu_presentation(&mut self) {
        // Clear every derived highlight first. This is linear in menu surfaces and avoids retaining
        // a second open-path projection beside the forest's authoritative active leaf.
        for index in 0..self.surfaces.nodes.len() {
            let node = &mut self.surfaces.nodes[index];
            if let Some(root) = node.root_mut()
                && let Some(bar) = root.menu_bar.as_mut()
            {
                bar.set_open_slot(None);
            }
            if let Some(menu) = node.surface.body.menu_mut() {
                menu.set_open_slot(None);
            }
        }

        // Each active menu popup marks exactly the trigger slot in its direct parent surface.
        for index in 0..self.surfaces.visible_order.len() {
            let key = self.surfaces.visible_order[index];
            let SurfaceKey::Popup(popup) = key else { continue };
            let relation = {
                let Some(node) = self.surfaces.popup_node(popup) else { continue };
                let Some(parent) = node.parent else { continue };
                let PopupState::Menu { trigger_slot } = node.popup().expect("popup key must retain popup policy") else {
                    continue;
                };
                (parent, *trigger_slot)
            };
            let (parent, trigger_slot) = relation;
            if let Some(source) = self.menu_surface_mut(parent) {
                source.set_open_slot(Some(trigger_slot));
            }
        }
    }

    /// Resolves one active popup rectangle from its exact or relational placement.
    fn resolved_popup_rect(&self, key: SurfaceKey) -> Option<Recti> {
        let SurfaceKey::Popup(popup) = key else { return None };
        let definition = self.surfaces.popup_node(popup)?;
        match definition.popup()? {
            PopupState::Application { anchor, .. } => Some(*anchor),
            PopupState::Menu { trigger_slot } => {
                // The direct parent is the sole anchor source: root parents open below, while popup
                // parents open to the right. No runtime node or weak typed widget is consulted.
                let parent = definition.parent?;
                let below = matches!(parent, SurfaceKey::Root(_));
                let trigger = self.menu_surface(parent)?.slot_rect(*trigger_slot)?;
                let current = definition.surface.rect;
                let (x, y) = if below {
                    (trigger.x, trigger.y.saturating_add(trigger.height))
                } else {
                    (trigger.x.saturating_add(trigger.width), trigger.y)
                };
                Some(Recti::new(x, y, current.width, current.height))
            }
        }
    }

    /// Revokes competing pointer gestures without clearing any scope's remembered keyboard focus.
    fn clear_other_pointer_captures(&mut self, keep: SurfaceKey) {
        for node in &mut self.surfaces.nodes {
            if node.key != keep {
                node.clear_pointer_targets();
            }
        }
        // A root menu bar and its application body share one forest key, so revoke same-root widget
        // capture explicitly while leaving its focus identity intact.
        if let Some(node) = self.surfaces.node_mut(keep) {
            node.surface.body.clear_pointer_targets();
        }
    }

    /// Returns whether the concrete menu attached to a visible key contains one pointer point.
    fn menu_contains(&self, key: SurfaceKey, point: Vec2i) -> bool {
        self.menu_surface(key).is_some_and(|menu| menu.contains(point))
    }

    /// Routes a pointer event directly to a root bar or menu-popup body.
    fn route_menu_pointer(&mut self, key: SurfaceKey, event: &UiInputEvent) -> Option<crate::menu::MenuRoute> {
        self.menu_surface_mut(key).map(|menu| menu.route_pointer(event))
    }

    /// Returns the direct menu-popup child opened by one branch slot.
    fn menu_popup_for_slot(&self, surface: SurfaceKey, slot: usize) -> Option<PopupId> {
        // Forest parent edges are authoritative, so no parallel submenu map is needed for keyboard
        // or pointer activation.
        self.surfaces
            .nodes
            .iter()
            .find(|node| node.parent == Some(surface) && matches!(node.popup(), Some(PopupState::Menu { trigger_slot }) if *trigger_slot == slot))
            .and_then(|node| match node.key {
                SurfaceKey::Popup(popup) => Some(popup),
                SurfaceKey::Root(_) => None,
            })
    }

    /// Clears every derived keyboard selection and releases the manager-owned menu scope.
    fn finish_keyboard_menu(&mut self, dismiss_popups: bool) {
        if dismiss_popups {
            // Item submission and explicit Alt/F10 cancellation close the complete menu ancestry.
            self.dismiss_active_popups();
        }
        for node in &mut self.surfaces.nodes {
            if let Some(root) = node.root_mut()
                && let Some(bar) = root.menu_bar.as_mut()
            {
                bar.clear_keyboard_target();
            }
            if let Some(menu) = node.surface.body.menu_mut() {
                menu.clear_keyboard_target();
            }
        }
        self.keyboard_menu_root = None;
        self.pending_menu_alt = false;
    }

    /// Opens one branch popup and selects an edge item for continued keyboard navigation.
    fn open_menu_slot(&mut self, surface: SurfaceKey, slot: usize, select_last: bool) -> bool {
        let Some(target) = self.menu_popup_for_slot(surface, slot) else {
            return false;
        };
        let _ = self.menu_surface_mut(surface).is_some_and(|menu| menu.focus_slot(slot));
        if self.open_popup_id(target).is_err() {
            return false;
        }
        let child = self
            .menu_surface_mut(SurfaceKey::Popup(target))
            .expect("a menu branch must own a compact menu-popup surface");
        if select_last { child.focus_last() } else { child.focus_first() }
    }

    /// Returns the current menu keyboard surface for one active root scope.
    fn keyboard_menu_surface(&self, root: RootId) -> SurfaceKey {
        // The deepest active menu popup is the current scope. With no popup, the intrinsic bar owns
        // navigation; an unrelated application popup is excluded by session validation.
        if let Some(popup) = self.surfaces.active_popup
            && self.surfaces.owning_root(SurfaceKey::Popup(popup)) == Some(root)
            && matches!(self.surfaces.popup_node(popup).and_then(SurfaceNode::popup), Some(PopupState::Menu { .. }))
        {
            SurfaceKey::Popup(popup)
        } else {
            SurfaceKey::Root(root)
        }
    }

    /// Enters the active window's intrinsic menu bar while preserving application widget focus.
    fn begin_keyboard_menu(&mut self) -> bool {
        let Some(SurfaceKey::Root(root)) = self.keyboard_input_surface() else {
            return false;
        };
        if !self
            .surfaces
            .root_node(root)
            .and_then(SurfaceNode::root)
            .and_then(|state| state.menu_bar.as_ref())
            .is_some()
        {
            return false;
        }

        // An application popup and an intrinsic menu cannot share the sole transient branch.
        // Dismiss it before borrowing the bar and installing the first selection.
        self.dismiss_active_popups();
        let selected = self.menu_surface_mut(SurfaceKey::Root(root)).is_some_and(MenuSurface::focus_first);
        if selected {
            self.keyboard_menu_root = Some(root);
        }
        selected
    }

    /// Moves the selected bar heading and optionally replaces the open top-level popup.
    fn move_keyboard_menu_heading(&mut self, root: RootId, forward: bool, open: bool) -> bool {
        let bar = SurfaceKey::Root(root);
        let Some(menu) = self.menu_surface_mut(bar) else {
            return false;
        };
        if !menu.move_keyboard_focus(forward) {
            return false;
        }
        let Some(slot) = menu.keyboard_slot() else { return false };
        !open || self.open_menu_slot(bar, slot, false)
    }

    /// Closes one popup level and restores selection to its parent trigger.
    fn close_keyboard_menu_level(&mut self, root: RootId) -> bool {
        let Some(popup) = self.surfaces.active_popup else {
            self.finish_keyboard_menu(false);
            return true;
        };
        let Some(node) = self.surfaces.popup_node(popup) else {
            self.finish_keyboard_menu(true);
            return true;
        };
        let Some(parent) = node.parent else {
            self.finish_keyboard_menu(true);
            return true;
        };
        let PopupState::Menu { trigger_slot } = node.popup().expect("popup key must retain popup policy") else {
            self.finish_keyboard_menu(true);
            return true;
        };
        let trigger_slot = *trigger_slot;
        let depth = self.surfaces.popup_depth(popup).expect("active menu popup must retain rooted ancestry");
        self.truncate_active_popup_path(depth);
        let _ = self.menu_surface_mut(parent).is_some_and(|menu| menu.focus_slot(trigger_slot));
        self.keyboard_menu_root = Some(root);
        true
    }

    /// Applies a key press inside the current bar or popup scope.
    fn apply_keyboard_menu_key(&mut self, root: RootId, event: crate::KeyEvent) -> bool {
        if !event.is_pressed() {
            // All releases remain manager-owned while the menu scope is active.
            return true;
        }
        let surface = self.keyboard_menu_surface(root);
        let popup = matches!(surface, SurfaceKey::Popup(_));
        match event.key {
            crate::Key::ArrowDown if popup => self.menu_surface_mut(surface).is_some_and(|menu| menu.move_keyboard_focus(true)),
            crate::Key::ArrowUp if popup => self.menu_surface_mut(surface).is_some_and(|menu| menu.move_keyboard_focus(false)),
            crate::Key::Home => self.menu_surface_mut(surface).is_some_and(MenuSurface::focus_first),
            crate::Key::End => self.menu_surface_mut(surface).is_some_and(MenuSurface::focus_last),
            crate::Key::ArrowRight if !popup => self.move_keyboard_menu_heading(root, true, false),
            crate::Key::ArrowLeft if !popup => self.move_keyboard_menu_heading(root, false, false),
            crate::Key::ArrowDown if !popup => {
                let Some(slot) = self.menu_surface(surface).and_then(MenuSurface::keyboard_slot) else {
                    return true;
                };
                self.open_menu_slot(surface, slot, false)
            }
            crate::Key::Enter | crate::Key::Space if !popup && !event.repeat => {
                let Some(slot) = self.menu_surface(surface).and_then(MenuSurface::keyboard_slot) else {
                    return true;
                };
                self.open_menu_slot(surface, slot, false)
            }
            crate::Key::ArrowUp if !popup => {
                let Some(slot) = self.menu_surface(surface).and_then(MenuSurface::keyboard_slot) else {
                    return true;
                };
                self.open_menu_slot(surface, slot, true)
            }
            crate::Key::ArrowRight if popup => match self.menu_surface(surface).and_then(MenuSurface::keyboard_branch_slot) {
                Some(slot) => self.open_menu_slot(surface, slot, false),
                None if self.surfaces.popup_depth(match surface {
                    SurfaceKey::Popup(id) => id,
                    SurfaceKey::Root(_) => unreachable!(),
                }) == Some(0) =>
                {
                    self.move_keyboard_menu_heading(root, true, true)
                }
                None => true,
            },
            crate::Key::ArrowLeft if popup => {
                let SurfaceKey::Popup(popup) = surface else { unreachable!() };
                if self.surfaces.popup_depth(popup) == Some(0) {
                    self.move_keyboard_menu_heading(root, false, true)
                } else {
                    self.close_keyboard_menu_level(root)
                }
            }
            crate::Key::Enter | crate::Key::Space if popup && !event.repeat => {
                let action = self.menu_surface_mut(surface).and_then(MenuSurface::activate_keyboard_slot);
                match action {
                    Some(MenuAction::OpenSlot(slot)) => self.open_menu_slot(surface, slot, false),
                    Some(MenuAction::SubmitAndClose) => {
                        self.finish_keyboard_menu(true);
                        true
                    }
                    None => true,
                }
            }
            crate::Key::Escape => self.close_keyboard_menu_level(root),
            _ => true,
        }
    }

    /// Handles Alt/F10 scope transitions and active menu navigation before widget key routing.
    fn route_menu_keyboard(&mut self, event: &UiInputEvent) -> bool {
        let UiInputEvent::Key { event } = event else {
            // Text input belongs to neither mnemonics nor type-ahead in this MVP, but an active
            // menu still consumes it so suspended application focus cannot edit behind the menu.
            return self.keyboard_menu_root.is_some() && matches!(event, UiInputEvent::Text { .. });
        };

        if event.key == crate::Key::Alt {
            if event.is_pressed() {
                self.pending_menu_alt = !event.repeat;
            } else if std::mem::take(&mut self.pending_menu_alt) {
                if self.keyboard_menu_root.is_some() {
                    self.finish_keyboard_menu(true);
                } else {
                    let _ = self.begin_keyboard_menu();
                }
            }
            return true;
        }
        if event.is_pressed() && self.pending_menu_alt {
            // Any chorded key cancels Alt-tap activation; this event may continue to a focused
            // widget when no keyboard menu is already active.
            self.pending_menu_alt = false;
        }

        let plain_f10 = event.key == crate::Key::Function(10) && event.modifiers == crate::Modifiers::NONE;
        if plain_f10 {
            if event.is_pressed() && !event.repeat {
                if self.keyboard_menu_root.is_some() {
                    self.finish_keyboard_menu(true);
                } else {
                    let _ = self.begin_keyboard_menu();
                }
            }
            return true;
        }

        let Some(root) = self.keyboard_menu_root else {
            return false;
        };
        self.apply_keyboard_menu_key(root, *event)
    }

    /// Dismisses the active application popup with Escape and owns the complete key transition.
    fn route_application_popup_keyboard(&mut self, event: &UiInputEvent) -> bool {
        let UiInputEvent::Key { event } = event else {
            return false;
        };
        if event.key != crate::Key::Escape {
            return false;
        }

        if self.popup_escape_key_down {
            // The press already removed the popup. Keep its physical release out of the restored
            // parent runtime, then close this small manager-owned command boundary.
            if !event.is_pressed() {
                self.popup_escape_key_down = false;
            }
            return true;
        }
        let Some(SurfaceKey::Popup(popup)) = self.active_surface else {
            return false;
        };
        if !self.surfaces.popup_is_active(popup)
            || !matches!(
                self.surfaces.popup_node(popup).and_then(SurfaceNode::popup),
                Some(PopupState::Application { .. })
            )
        {
            return false;
        }

        if event.is_pressed() {
            self.popup_escape_key_down = true;
            if !event.repeat {
                let depth = self.surfaces.popup_depth(popup).expect("active application popup must retain rooted ancestry");
                self.truncate_active_popup_path(depth);
            }
        }
        true
    }

    /// Handles the Windows-style Ctrl+F6 window-cycle command before widget key routing.
    fn route_window_keyboard(&mut self, event: &UiInputEvent) -> bool {
        let UiInputEvent::Key { event } = event else {
            return false;
        };
        if event.key != crate::Key::Function(6) {
            return false;
        }

        // Once a qualifying press is accepted, every later F6 transition remains manager-owned
        // until release. This closes the command boundary even if Control is released first.
        if self.window_cycle_key_down {
            if !event.is_pressed() {
                self.window_cycle_key_down = false;
            }
            return true;
        }
        let forward_modifiers = crate::Modifiers::CTRL;
        let reverse_modifiers = crate::Modifiers::CTRL | crate::Modifiers::SHIFT;
        let is_cycle_chord = event.modifiers == forward_modifiers || event.modifiers == reverse_modifiers;
        if !is_cycle_chord {
            return false;
        }
        if !event.is_pressed() {
            // Consume an isolated matching release defensively without inventing an activation.
            return true;
        }
        self.window_cycle_key_down = true;

        // Repeated presses belong to the accepted physical chord but do not race through windows.
        // An active intrinsic menu or modal dialog likewise retains its current keyboard scope.
        if event.repeat || self.keyboard_menu_root.is_some() || self.surfaces.active_modal_root().is_some() {
            return true;
        }

        // Resolve the source before closing an application popup because that popup may currently
        // own capture and therefore identify the active ordinary window. Each candidate runtime
        // remains mounted, preserving its remembered widget focus across the switch.
        let current = self.keyboard_input_surface().and_then(|surface| self.surfaces.owning_root(surface));
        self.dismiss_active_popups();
        let Some(target) = self.surfaces.window_cycle_target(current, event.modifiers == reverse_modifiers) else {
            return true;
        };
        self.active_surface = Some(SurfaceKey::Root(target));
        self.bring_window_to_front_id(target)
            .expect("a visible cycle target collected from the forest must remain registered");
        true
    }

    /// Applies one pointer-produced menu action after releasing the originating surface borrow.
    fn apply_menu_action(&mut self, surface: SurfaceKey, action: MenuAction, previous_top: Option<PopupId>) {
        match action {
            MenuAction::OpenSlot(slot) => {
                let target = self
                    .menu_popup_for_slot(surface, slot)
                    .expect("a menu branch slot must retain one direct popup child");
                // Outside dismissal already closed the old path before this bar press reached the
                // window. A repeated heading press therefore closes the keyboard scope as well.
                if matches!(surface, SurfaceKey::Root(_)) && previous_top == Some(target) {
                    self.finish_keyboard_menu(false);
                    return;
                }
                let owner = self.surfaces.owning_root(surface).expect("a menu surface must retain one root owner");
                self.keyboard_menu_root = Some(owner);
                let _ = self.open_menu_slot(surface, slot, false);
            }
            MenuAction::SubmitAndClose => {
                // The surface already queued the typed event. Arm tail suppression before removing
                // its popup so the physical release cannot fall through to newly exposed content.
                self.discard_pointer_capture_tail = true;
                self.finish_keyboard_menu(true);
            }
        }
    }

    /// Routes one event to window chrome and reports whether the overlay consumed it.
    fn route_chrome_event(&mut self, root: RootId, event: &UiInputEvent) -> bool {
        if self.root_node(root).is_err() {
            return false;
        }
        match event {
            UiInputEvent::MouseDown { pos, button } if button.intersects(MouseButton::LEFT) => {
                match self.root_node(root).ok().and_then(|node| node.chrome_part_at(*pos)) {
                    Some(RootChromePart::Close) => {
                        // Apply visibility before queuing Close so subscribers observe final policy.
                        self.set_window_visible_id(root, false).expect("chrome target must remain registered");
                        self.root_node_mut(root)
                            .expect("closing a root must not destroy it")
                            .emit_window_event(WindowEvent::CloseRequested);
                        true
                    }
                    Some(RootChromePart::Resize) => {
                        let node = self.root_node_mut(root).expect("chrome target must remain retained");
                        node.surface.body.clear_transient_targets();
                        node.root_mut().expect("chrome target must be a root").interaction = RootInteraction::Resizing;
                        true
                    }
                    Some(RootChromePart::Title) => {
                        let node = self.root_node_mut(root).expect("chrome target must remain retained");
                        node.surface.body.clear_transient_targets();
                        node.root_mut().expect("chrome target must be a root").interaction = RootInteraction::Moving;
                        true
                    }
                    None => false,
                }
            }
            UiInputEvent::MouseDrag { pos, delta, .. } => {
                let node = self.root_node_mut(root).expect("drag target must remain retained");
                let initial = node.surface.rect;
                match node.root().expect("drag target must be a root").interaction {
                    RootInteraction::Moving => {
                        node.surface.rect.x = initial.x.saturating_add(delta.x);
                        node.surface.rect.y = initial.y.saturating_add(delta.y);
                    }
                    RootInteraction::Resizing => {
                        let minimum = node.surface.geometry.minimum_outer;
                        node.surface.rect.width = initial.width.saturating_add(delta.x).max(minimum.width);
                        node.surface.rect.height = initial.height.saturating_add(delta.y).max(minimum.height);
                    }
                    RootInteraction::None => return node.chrome_part_at(*pos).is_some(),
                }
                if (node.surface.rect.x, node.surface.rect.y, node.surface.rect.width, node.surface.rect.height)
                    != (initial.x, initial.y, initial.width, initial.height)
                {
                    // Snapshot after geometry mutation so subscribers receive the committed value.
                    let rect = node.surface.rect;
                    node.emit_window_event(WindowEvent::GeometryChanged { rect });
                    self.invalidate_ui_commit();
                }
                true
            }
            UiInputEvent::MouseUp { button, .. }
                if button.intersects(MouseButton::LEFT)
                    && self
                        .root_node(root)
                        .ok()
                        .and_then(SurfaceNode::root)
                        .is_some_and(|state| state.interaction != RootInteraction::None) =>
            {
                self.root_node_mut(root)
                    .expect("release target must remain retained")
                    .root_mut()
                    .expect("release target must be a root")
                    .interaction = RootInteraction::None;
                true
            }
            _ => event
                .position()
                .is_some_and(|point| self.root_node(root).ok().and_then(|node| node.chrome_part_at(point)).is_some()),
        }
    }

    /// Performs one synchronization layout, then one update/layout pair per queued event.
    pub(crate) fn update(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        // Polling contexts have no application dispatcher, so the safe boundary performs no work.
        self.update_with(dimensions, atlas, &mut (), |_, _| false);
    }

    /// Performs retained updates while exposing each safe subscriber-dispatch boundary.
    pub(crate) fn update_with<DispatchState>(
        &mut self,
        dimensions: Dimensioni,
        atlas: &crate::AtlasHandle,
        dispatch_state: &mut DispatchState,
        mut after_event: impl FnMut(&mut Self, &mut DispatchState) -> bool,
    ) {
        self.ui_commit = None;
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        for node in &mut self.surfaces.nodes {
            // Every concrete surface begins the cycle so hidden trees retain coherent staged state.
            node.surface.body.begin_update();
        }
        self.layout(viewport, atlas);
        if after_event(self, dispatch_state) {
            self.layout(viewport, atlas);
        }

        while let Some(event) = self.input.pop_event() {
            let input = self.input.snapshot();
            self.update_for_event(atlas, &event, input);
            // Subscribers run after all retained borrows are released and before the next layout.
            after_event(self, dispatch_state);
            self.layout(viewport, atlas);
        }
        self.ui_commit = Some(dimensions);
    }

    /// Resolves the effective screen-space clip for one visible surface during parent-first layout.
    fn resolved_surface_clip(&self, key: SurfaceKey, viewport: Recti) -> Recti {
        match key {
            SurfaceKey::Root(root) => {
                let node = self.surfaces.root_node(root).expect("visible root must remain retained");
                let state = node.root().expect("root key must retain root policy");
                if state.mode != RootMode::Child {
                    return viewport;
                }

                // A child inherits the complete boundary already committed by its direct parent.
                // Content clipping narrows that inherited rectangle once at this edge; deeper
                // children naturally accumulate every clipping ancestor through the same rule.
                let parent = match node.parent {
                    Some(SurfaceKey::Root(parent)) => parent,
                    Some(SurfaceKey::Popup(_)) | None => unreachable!("a child window must retain one root parent"),
                };
                let parent = self.surfaces.root_node(parent).expect("visible child parent must remain retained");
                let inherited = parent.surface.clip;
                match parent.root().expect("child parent must retain root policy").child_window_clip {
                    ChildWindowClip::None => inherited,
                    ChildWindowClip::Content => inherited.intersect(&parent.surface.geometry.body).unwrap_or_else(|| {
                        Recti::new(
                            inherited.x.max(parent.surface.geometry.body.x),
                            inherited.y.max(parent.surface.geometry.body.y),
                            0,
                            0,
                        )
                    }),
                }
            }
            SurfaceKey::Popup(_) => {
                // Popups inherit their owning window's surface boundary rather than its child-body
                // boundary. A parent's own menu can therefore overlay its children, while a popup
                // owned by a clipped child remains confined by that child's ancestors.
                let owner = self.surfaces.owning_root(key).expect("visible popup must retain a root owner");
                self.surfaces.root_node(owner).expect("visible popup owner must remain retained").surface.clip
            }
        }
    }

    /// Lays out the reusable parent-first visible traversal and commits inherited surface clips.
    fn layout(&mut self, viewport: Recti, atlas: &crate::AtlasHandle) {
        self.sync_menu_presentation();
        self.surfaces.rebuild_visible_order();
        let style = self.style;
        for index in 0..self.surfaces.nodes.len() {
            let key = self.surfaces.nodes[index].key;
            if !self.surfaces.visible_order.contains(&key) {
                self.surfaces.nodes[index].clear_transient_targets();
            }
        }

        // Keys are copied one at a time so mutating geometry never requires cloning the traversal.
        for index in 0..self.surfaces.visible_order.len() {
            let key = self.surfaces.visible_order[index];
            if matches!(key, SurfaceKey::Popup(_)) {
                let rect = self
                    .resolved_popup_rect(key)
                    .expect("active popup anchor node must remain in its retained parent surface");
                self.surfaces.surface_mut(key).expect("visible popup must remain retained").rect = rect;
            }
            let clip = self.resolved_surface_clip(key, viewport);
            let node = self.surfaces.node_mut(key).expect("visible surface must remain retained");
            node.layout(&style, atlas, clip);
            if node.take_popup_focus_request() {
                // The popup is now mounted with committed geometry, so its first eligible Tab stop
                // can become the complete root-owned focus path without a speculative rectangle.
                let _ = node.surface.body.advance_focus(false);
            }
        }
    }

    /// Routes and applies one normalized event across every eligible surface.
    fn update_for_event(&mut self, atlas: &crate::AtlasHandle, event: &UiInputEvent, input: crate::input::InputSnapshot) {
        // Resolve event-wide dismissal and menu-toggle context before selecting a recipient. An
        // outside press may change the visible forest and must do so before hit testing below.
        let style = self.style;
        let popup_keyboard_handled = self.route_application_popup_keyboard(event);
        let menu_keyboard_handled = !popup_keyboard_handled && self.route_menu_keyboard(event);
        let window_keyboard_handled = !popup_keyboard_handled && !menu_keyboard_handled && self.route_window_keyboard(event);
        let discard_pointer = self.discard_revoked_capture_event(event);
        let menu_press = matches!(event, UiInputEvent::MouseDown { button, .. } if button.intersects(MouseButton::LEFT));
        let previous_top = menu_press.then(|| self.surfaces.active_top_popup()).flatten();
        if matches!(event, UiInputEvent::MouseDown { .. }) {
            // Dismiss before target resolution so the same outside press reaches the revealed surface.
            self.dismiss_outside_popup(input.mouse_pos);
            // Any capture revoked by outside dismissal belonged to an older gesture. This fresh
            // press is intentionally routed to the revealed surface and owns its own later tail.
            self.discard_pointer_capture_tail = false;
        }

        // Select and activate the topmost eligible pointer surface from the newly visible order.
        let hover = (event.is_pointer() && !discard_pointer)
            .then(|| self.input_surface_at(input.mouse_pos))
            .flatten();
        if matches!(event, UiInputEvent::MouseDown { .. })
            && self.keyboard_menu_root.is_some()
            && !hover.is_some_and(|surface| self.menu_contains(surface, input.mouse_pos))
        {
            // The outside press already truncated the popup path above. Release only menu keyboard
            // selection here so the same click can continue into application content.
            self.finish_keyboard_menu(false);
        }
        if matches!(event, UiInputEvent::MouseDown { .. })
            && let Some(surface) = hover
        {
            self.activate_pointer_surface(surface);
            let owner = self.surfaces.owning_root(surface).expect("visible surface must retain a root ancestor");
            self.bring_window_to_front_id(owner).expect("pointer target owner must remain registered");
            // A fresh gesture preempts every competing pointer capture but never any scope's
            // remembered keyboard focus. The selected focusable body target may replace its own
            // focus during routing below.
            self.clear_other_pointer_captures(surface);
        }

        let drag = (!discard_pointer).then(|| self.drag_input_surface()).flatten();
        let pointer = match event {
            UiInputEvent::MouseDrag { .. } => drag,
            _ => hover,
        };
        let keyboard = self.keyboard_input_surface();
        let tab_navigation = matches!(
            event,
            UiInputEvent::Key { event }
                if event.key == crate::Key::Tab
                    && !event
                        .modifiers
                        .intersects(crate::Modifiers::ALT | crate::Modifiers::CTRL | crate::Modifiers::SUPER)
        );
        let focus_traversal = tab_navigation
            .then(|| match event {
                UiInputEvent::Key { event } if event.is_pressed() => Some(event.modifiers.intersects(crate::Modifiers::SHIFT)),
                _ => None,
            })
            .flatten();
        let modal = self.surfaces.active_modal_root();

        // Stage pointer eligibility in every participating widget runtime before routing one target.
        for index in 0..self.surfaces.visible_order.len() {
            let key = self.surfaces.visible_order[index];
            if self.surface_is_eligible(key, modal) {
                let widget_pointer = pointer == Some(key) && !self.menu_contains(key, input.mouse_pos);
                self.surfaces
                    .surface_mut(key)
                    .expect("visible input surface must remain retained")
                    .body
                    .begin_input_event(widget_pointer, event);
            }
        }

        // Captured gestures take priority; otherwise route exactly one pointer hit or keyboard
        // target. Menu policy is staged until its concrete surface borrow has been released.
        let mut staged_menu_action = None;
        if event.is_pointer() && !discard_pointer {
            let captured = matches!(event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. })
                .then(|| self.captured_input_surface())
                .flatten();
            let mut handled = false;
            if let Some(surface) = captured {
                if self.menu_surface(surface).is_some_and(MenuSurface::has_capture) {
                    let route = self.route_menu_pointer(surface, event).expect("captured menu surface must remain retained");
                    staged_menu_action = route.action.map(|action| (surface, action));
                    handled = route.handled;
                } else {
                    handled = match surface {
                        SurfaceKey::Root(root) => {
                            // Chrome receives continuation only when it owns the window's capture. A
                            // captured application widget must keep drag and release even over chrome.
                            let chrome_captured = self
                                .root_node(root)
                                .ok()
                                .and_then(SurfaceNode::root)
                                .is_some_and(|state| state.interaction != RootInteraction::None);
                            if chrome_captured {
                                self.route_chrome_event(root, event)
                            } else {
                                self.surfaces
                                    .surface_mut(surface)
                                    .expect("captured window must remain retained")
                                    .body
                                    .route_captured_pointer(&style, input.mouse_buttons, event)
                                    .is_some()
                            }
                        }
                        SurfaceKey::Popup(_) => self
                            .surfaces
                            .surface_mut(surface)
                            .expect("captured popup must remain retained")
                            .body
                            .route_captured_pointer(&style, input.mouse_buttons, event)
                            .is_some(),
                    };
                }
            }
            if !handled && let Some(surface) = pointer {
                handled = matches!(surface, SurfaceKey::Root(root) if self.route_chrome_event(root, event));
                if !handled && self.menu_contains(surface, input.mouse_pos) {
                    let route = self.route_menu_pointer(surface, event).expect("hit menu surface must remain retained");
                    staged_menu_action = route.action.map(|action| (surface, action));
                    handled = route.handled;
                }
                if !handled {
                    let body = &mut self.surfaces.surface_mut(surface).expect("pointer surface must remain retained").body;
                    if body.accepts_pointer_input() {
                        let _ = body.route_pointer(&style, event, input.mouse_buttons);
                    }
                }
            }
        } else if popup_keyboard_handled || menu_keyboard_handled || window_keyboard_handled {
            // Manager keyboard routing mutated only scope and compact-surface state. The ordinary
            // full-tree update below still lets suspended application widgets observe stable focus.
        } else if tab_navigation {
            // Both transitions belong to the scope command, so neither Tab press nor release leaks
            // into a newly focused widget. Only the press advances; a held key may still advance
            // through ordinary repeated press events supplied by the platform.
            if let (Some(surface), Some(reverse)) = (keyboard, focus_traversal) {
                self.surfaces
                    .surface_mut(surface)
                    .expect("keyboard surface must remain retained")
                    .body
                    .advance_focus(reverse);
            }
        } else if event.is_focus_input()
            && let Some(surface) = keyboard
        {
            self.surfaces
                .surface_mut(surface)
                .expect("keyboard surface must remain retained")
                .body
                .route_focus(&style, event);
        }

        // Reconcile transient state for surfaces excluded by the routed event, then update every
        // eligible body in the shared visible order using the same immutable input snapshot.
        let modal = self.surfaces.active_modal_root();
        for index in 0..self.surfaces.nodes.len() {
            let key = self.surfaces.nodes[index].key;
            let visible = self.surfaces.visible_order.contains(&key);
            if !visible {
                self.surfaces.nodes[index].clear_transient_targets();
            } else if !self.surface_is_eligible(key, modal) {
                // A modal scope suspends underlying keyboard focus rather than destroying it.
                // Pointer state cannot survive the exclusion and is revoked independently.
                self.surfaces.nodes[index].clear_pointer_targets();
            }
        }
        for index in 0..self.surfaces.visible_order.len() {
            let key = self.surfaces.visible_order[index];
            if self.surface_is_eligible(key, modal) {
                self.surfaces
                    .surface_mut(key)
                    .expect("visible update surface must remain retained")
                    .body
                    .update(&style, atlas.clone(), input);
            }
        }
        self.reconcile_active_surface();
        if let Some((surface, action)) = staged_menu_action {
            // Direct routing has ended its concrete surface borrow before forest visibility changes.
            self.apply_menu_action(surface, action, previous_top);
        }
    }

    /// Records one ordinary root family with each parent's application content below its children.
    fn paint_root_tree(&mut self, root: RootId, style: &Style, atlas: &crate::AtlasHandle, focus_surface: Option<SurfaceKey>, active_window: Option<RootId>) {
        {
            // A short node borrow records the base before recursion. Releasing it here lets direct
            // child calls borrow arbitrary later forest entries without unsafe aliasing or mirrors.
            let node_index = self.surfaces.node_index(SurfaceKey::Root(root)).expect("visible root must remain retained");
            let node = &mut self.surfaces.nodes[node_index];
            record_root_background(&mut self.display_list, node.surface.clip, node.surface.rect, node.surface.options, style);
            node.surface
                .body
                .paint(&mut self.display_list, style, atlas, focus_surface == Some(SurfaceKey::Root(root)));
        }

        // Forest chronology remains the sibling z-order source. Scanning direct edges avoids a
        // second child collection while recursion keeps each descendant family contiguous.
        let node_count = self.surfaces.nodes.len();
        for index in 0..node_count {
            let child = {
                let node = &self.surfaces.nodes[index];
                match (node.key, node.parent, node.root()) {
                    (SurfaceKey::Root(child), Some(SurfaceKey::Root(parent)), Some(state))
                        if parent == root && state.mode == RootMode::Child && state.visible =>
                    {
                        Some(child)
                    }
                    _ => None,
                }
            };
            if let Some(child) = child {
                self.paint_root_tree(child, style, atlas, focus_surface, active_window);
            }
        }

        {
            // Parent-owned presentation is deliberately delayed until every child body and child
            // overlay has recorded. The inverse input recursion below uses the same relationship.
            let node_index = self.surfaces.node_index(SurfaceKey::Root(root)).expect("visible root must remain retained");
            let node = &mut self.surfaces.nodes[node_index];
            if let Some(root) = node.root_mut()
                && let Some(bar) = root.menu_bar.as_mut()
            {
                bar.paint(&mut self.display_list, style, atlas);
            }
            record_root_overlay(
                &mut self.display_list,
                node.surface.clip,
                node.surface.rect,
                node.surface.options,
                &node.surface.name,
                node.surface.geometry,
                style,
                atlas,
                active_window == Some(root),
            );
        }
    }

    /// Records the sole active popup path in parent-to-child order at its inherited transient tier.
    fn paint_active_popup_path(&mut self, style: &Style, atlas: &crate::AtlasHandle, focus_surface: Option<SurfaceKey>) {
        // `visible_order` materializes only the active popup ancestry. Filtering it reuses that
        // allocation and preserves the existing parent-first popup paint contract.
        for index in 0..self.surfaces.visible_order.len() {
            let key = self.surfaces.visible_order[index];
            if !matches!(key, SurfaceKey::Popup(_)) {
                continue;
            }
            let node_index = self.surfaces.node_index(key).expect("active popup must remain retained");
            let node = &mut self.surfaces.nodes[node_index];
            record_root_background(&mut self.display_list, node.surface.clip, node.surface.rect, node.surface.options, style);
            node.surface.body.paint(&mut self.display_list, style, atlas, focus_surface == Some(key));
        }
    }

    /// Paints fixed window families, their transient tier, and then the manager-owned modal tier.
    pub(crate) fn paint(&mut self, _dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        // Frame validation has already matched `_dimensions` to the latest UI commit. Painting uses
        // the committed per-surface clips from that transaction so no traversal can accidentally
        // widen a structurally clipped child back to the full drawable viewport.
        self.display_list.clear();
        self.surfaces.rebuild_visible_order();
        let style = self.style;
        let active_mode = self.active_popup_owner().and_then(|owner| self.surfaces.stacking_mode(owner));
        // A menu owns keyboard presentation without discarding application focus. Otherwise the
        // same surface selected by routing is the only runtime allowed to paint remembered focus.
        let focus_surface = if self.keyboard_menu_root.is_some() {
            None
        } else {
            self.keyboard_input_surface()
        };
        // Window activation follows the current modal/menu/keyboard scope and is independent from
        // fixed-layer stacking. Popup focus projects to its sole owning window for chrome paint.
        let active_window = self
            .keyboard_menu_root
            .or_else(|| focus_surface.and_then(|surface| self.surfaces.owning_root(surface)));

        // Only parentless Normal roots enter global layer scans. Each recursive call paints one
        // structurally atomic family while still sandwiching children between parent content and
        // parent-owned overlays.
        for layer in MIN_LAYER..=MAX_LAYER {
            let node_count = self.surfaces.nodes.len();
            for index in 0..node_count {
                let root = {
                    let node = &self.surfaces.nodes[index];
                    match (node.key, node.root()) {
                        (SurfaceKey::Root(root), Some(state))
                            if state.mode == (RootMode::Normal { layer }) && self.surfaces.root_is_effectively_visible(root) =>
                        {
                            Some(root)
                        }
                        _ => None,
                    }
                };
                if let Some(root) = root {
                    self.paint_root_tree(root, &style, atlas, focus_surface, active_window);
                }
            }
            if active_mode == Some(RootMode::Normal { layer }) {
                self.paint_active_popup_path(&style, atlas, focus_surface);
            }
        }

        let node_count = self.surfaces.nodes.len();
        for index in 0..node_count {
            let modal = {
                let node = &self.surfaces.nodes[index];
                match (node.key, node.root()) {
                    (SurfaceKey::Root(root), Some(state)) if state.mode == RootMode::Modal && self.surfaces.root_is_effectively_visible(root) => Some(root),
                    _ => None,
                }
            };
            if let Some(modal) = modal {
                self.paint_root_tree(modal, &style, atlas, focus_surface, active_window);
            }
        }
        if active_mode == Some(RootMode::Modal) {
            self.paint_active_popup_path(&style, atlas, focus_surface);
        }
    }

    /// Truncates an active popup branch according to one outside press.
    fn dismiss_outside_popup(&mut self, mouse: Vec2i) {
        if self.surfaces.active_popup.is_none() {
            return;
        }
        // Use the same front-surface resolution as pointer routing. Raw popup rectangles may be
        // occluded by a window in a higher layer and therefore cannot alone keep a popup open.
        let keep = match self.input_surface_at(mouse) {
            Some(SurfaceKey::Popup(popup)) if self.surfaces.popup_is_active(popup) => {
                self.surfaces.popup_depth(popup).map_or(0, |depth| depth.saturating_add(1))
            }
            _ => 0,
        };
        self.truncate_active_popup_path(keep);
    }

    /// Records the exact keyboard surface selected by one pointer press.
    fn activate_pointer_surface(&mut self, surface: SurfaceKey) {
        if self.surfaces.node(surface).is_some() {
            // A popup is not collapsed to its owner: its independent widget runtime must receive
            // subsequent Tab, key, and text transitions until focus leaves that surface.
            self.active_surface = Some(surface);
        }
    }

    /// Returns whether one retained window is visible.
    fn root_is_visible(&self, root: RootId) -> bool {
        // Keyboard activation and owner eligibility require the complete structural ancestry, not
        // merely the local visibility bit retained for restoration after a parent is shown again.
        self.surfaces.root_is_effectively_visible(root)
    }

    /// Tests the active popup path front-to-back and returns its first clipped surface hit.
    fn input_popup_at(&self, point: Vec2i) -> Option<SurfaceKey> {
        // Popup path order is parent-first for paint, so reverse traversal tests the deepest submenu
        // first. The sole active branch makes any additional ownership filtering unnecessary.
        self.surfaces
            .visible_order
            .iter()
            .rev()
            .copied()
            .filter(|key| matches!(key, SurfaceKey::Popup(_)))
            .find(|key| self.surfaces.surface(*key).is_some_and(|surface| surface.contains(point)))
    }

    /// Resolves one ordinary root family using the inverse of recursive paint order.
    fn input_root_tree_at(&self, root: RootId, point: Vec2i) -> Option<SurfaceKey> {
        let node = self.surfaces.root_node(root)?;
        if node.overlay_contains(point) {
            // Parent menus and actionable chrome paint after the complete descendant family and
            // therefore receive uncaptured input before any overlapping child surface.
            return Some(SurfaceKey::Root(root));
        }

        // Later sibling records paint in front. Recurse before testing the parent base so children
        // sit above application content while remaining below parent-owned overlays.
        for node in self.surfaces.nodes.iter().rev() {
            let (SurfaceKey::Root(child), Some(state)) = (node.key, node.root()) else {
                continue;
            };
            if state.mode == RootMode::Child
                && state.visible
                && node.parent == Some(SurfaceKey::Root(root))
                && let Some(target) = self.input_root_tree_at(child, point)
            {
                return Some(target);
            }
        }

        node.surface.contains(point).then_some(SurfaceKey::Root(root))
    }

    /// Returns the front eligible surface containing one pointer point.
    fn input_surface_at(&self, point: Vec2i) -> Option<SurfaceKey> {
        if let Some(modal) = self.surfaces.active_modal_root() {
            // The active modal and its popup path remain the sole eligible input group. Popup
            // surfaces occupy the transient tier above the modal root itself.
            if self.active_popup_owner() == Some(modal)
                && let Some(target) = self.input_popup_at(point)
            {
                return Some(target);
            }
            return self.input_root_tree_at(modal, point);
        }

        let active_owner = self.active_popup_owner();
        let active_mode = active_owner.and_then(|owner| self.surfaces.stacking_mode(owner));
        for layer in (MIN_LAYER..=MAX_LAYER).rev() {
            // A fixed band's popup path remains above every ordinary family in that same band but
            // below all higher fixed bands, preserving the established transient-tier policy.
            if active_mode == Some(RootMode::Normal { layer })
                && let Some(target) = self.input_popup_at(point)
            {
                return Some(target);
            }
            for node in self.surfaces.nodes.iter().rev() {
                let (SurfaceKey::Root(root), Some(state)) = (node.key, node.root()) else {
                    continue;
                };
                if state.mode == (RootMode::Normal { layer })
                    && self.surfaces.root_is_effectively_visible(root)
                    && let Some(target) = self.input_root_tree_at(root, point)
                {
                    return Some(target);
                }
            }
        }
        None
    }

    /// Returns the front visible surface in the current modal group.
    fn front_input_surface(&self) -> Option<SurfaceKey> {
        let modal = self.surfaces.active_modal_root();
        self.surfaces
            .visible_order
            .iter()
            .rev()
            .copied()
            .find(|key| self.surface_is_eligible(*key, modal))
    }

    /// Returns the eligible surface that currently owns pointer capture.
    fn captured_input_surface(&self) -> Option<SurfaceKey> {
        let modal = self.surfaces.active_modal_root();
        self.surfaces
            .visible_order
            .iter()
            .rev()
            .copied()
            .find(|key| self.surface_is_eligible(*key, modal) && self.surfaces.node(*key).is_some_and(SurfaceNode::has_capture))
    }

    /// Returns the surface receiving pointer-drag continuation.
    fn drag_input_surface(&self) -> Option<SurfaceKey> {
        self.captured_input_surface().or_else(|| self.front_input_surface())
    }

    /// Returns the sole surface receiving keyboard and text input.
    fn keyboard_input_surface(&self) -> Option<SurfaceKey> {
        let modal = self.surfaces.active_modal_root();
        self.captured_input_surface()
            .or_else(|| {
                self.active_surface
                    .filter(|surface| self.surfaces.visible_order.contains(surface) && self.surface_is_eligible(*surface, modal))
            })
            .or_else(|| modal.map(SurfaceKey::Root))
            .or_else(|| self.front_input_surface())
    }

    /// Returns the popup options installed for new definitions.
    const fn default_popup_options() -> WindowOption {
        WindowOption::FRAME
            .union(WindowOption::AUTO_SIZE)
            .union(WindowOption::NO_RESIZE)
            .union(WindowOption::NO_TITLE)
    }

    /// Returns visible surface names in base-surface paint order for tests.
    ///
    /// A structural parent's overlay is deliberately recorded after descendant base surfaces and
    /// therefore has no separate name entry in this diagnostic projection.
    #[cfg(test)]
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        self.surfaces
            .visible_order
            .iter()
            .filter_map(|key| self.surfaces.surface(*key).map(|surface| surface.name.clone()))
            .collect()
    }

    /// Returns one window name for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_name(&self, root: RootId) -> Option<String> {
        self.surfaces.root_node(root).map(|node| node.surface.name.clone())
    }

    /// Returns one window rectangle for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_rect(&self, root: RootId) -> Option<Recti> {
        self.surfaces.root_node(root).map(|node| node.surface.rect)
    }

    /// Returns the effective committed surface clip for one window hierarchy test.
    #[cfg(test)]
    pub(crate) fn debug_root_clip(&self, root: RootId) -> Option<Recti> {
        // Copy the geometry snapshot rather than exposing the mutable Surface that owns retained
        // runtime and chrome state.
        self.surfaces.root_node(root).map(|node| node.surface.clip)
    }

    /// Returns one window visibility value for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_visible(&self, root: RootId) -> Option<bool> {
        self.surfaces.root_node(root)?.root().map(|state| state.visible)
    }

    /// Returns whether window chrome owns a gesture for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_active(&self, root: RootId) -> Option<bool> {
        self.surfaces.root_node(root)?.root().map(|state| state.interaction != RootInteraction::None)
    }

    /// Returns whether window title movement is active for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_moving(&self, root: RootId) -> Option<bool> {
        self.surfaces.root_node(root)?.root().map(|state| state.interaction == RootInteraction::Moving)
    }

    /// Returns whether window resizing is active for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_resizing(&self, root: RootId) -> Option<bool> {
        self.surfaces
            .root_node(root)?
            .root()
            .map(|state| state.interaction == RootInteraction::Resizing)
    }

    /// Returns the root that owns the active concrete surface for tests.
    #[cfg(test)]
    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        self.active_surface.and_then(|surface| self.surfaces.owning_root(surface))
    }

    /// Returns the active modal dialog for tests.
    #[cfg(test)]
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.surfaces.active_modal_root()
    }

    /// Returns a window body rectangle for chrome geometry tests.
    #[cfg(test)]
    pub(crate) fn debug_root_body(&self, root: RootId, _atlas: &crate::AtlasHandle) -> Option<Recti> {
        let node = self.surfaces.root_node(root)?;
        // Committed geometry excludes the root-owned menu bar and therefore names app content.
        Some(node.surface.geometry.body)
    }

    /// Returns window runtime metrics for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_runtime_metrics(&self, root: RootId) -> Option<crate::ui_node::RuntimeMetrics> {
        self.surfaces.root_node(root)?.surface.body.metrics()
    }

    /// Returns combined window pointer capture for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        self.surfaces.root_node(root).map(SurfaceNode::has_capture)
    }

    /// Counts application nodes in one window for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_node_count(&self, root: RootId) -> Option<usize> {
        self.surfaces.root_node(root)?.surface.body.node_count()
    }

    /// Returns one application node rectangle inside a window.
    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.surfaces.root_node(root)?.surface.body.node_rect(node)
    }

    /// Returns title, close, and resize geometry for one window.
    #[cfg(test)]
    pub(crate) fn debug_root_chrome(&self, root: RootId, atlas: &crate::AtlasHandle) -> Option<(Option<Recti>, Option<Recti>, Option<Recti>)> {
        let node = self.surfaces.root_node(root)?;
        let geometry = root_chrome_geometry(
            node.surface.rect,
            Dimensioni::default(),
            &node.surface.name,
            node.surface.options,
            &self.style,
            atlas,
        );
        Some((geometry.title, geometry.close, geometry.resize))
    }

    /// Returns a popup rectangle through its typed handle for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_rect(&self, popup: &PopupHandle) -> Option<Recti> {
        Some(self.popup_node(popup).ok()?.surface.rect)
    }

    /// Returns whether a popup belongs to the sole active path for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_visible(&self, popup: &PopupHandle) -> Option<bool> {
        // Membership validation accepts the handle's private forest key without exposing it.
        Some(self.surfaces.popup_is_active(self.popup_id(popup).ok()?))
    }

    /// Returns popup content size through its typed handle for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_content_size(&self, popup: &PopupHandle) -> Option<Dimensioni> {
        self.popup_node(popup).ok()?.surface.body.content_size()
    }

    /// Returns one retained node rectangle inside a popup for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_node_rect(&self, popup: &PopupHandle, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.popup_node(popup).ok()?.surface.body.node_rect(node)
    }

    /// Returns the active popup names in parent-to-child order for path tests.
    #[cfg(test)]
    pub(crate) fn debug_active_popup_names(&self) -> Vec<String> {
        self.surfaces
            .visible_order
            .iter()
            .copied()
            .filter(|key| matches!(key, SurfaceKey::Popup(_)))
            .filter_map(|key| {
                let SurfaceKey::Popup(popup) = key else { return None };
                let node = self.surfaces.popup_node(popup)?;
                match node.popup()? {
                    PopupState::Application { .. } => Some(node.surface.name.clone()),
                    PopupState::Menu { trigger_slot } => {
                        let parent = node.parent?;
                        let label = self.menu_surface(parent)?.branch_label(*trigger_slot)?;
                        let owner = self.surfaces.owning_root(key)?;
                        let root_name = &self.surfaces.root_node(owner)?.surface.name;
                        Some(format!("{root_name} {label} Menu"))
                    }
                }
            })
            .collect()
    }

    /// Returns active popup rectangles in parent-to-child order for relational-anchor tests.
    #[cfg(test)]
    pub(crate) fn debug_active_popup_rects(&self) -> Vec<Recti> {
        // Copy geometry out so tests cannot mutate or retain references into window-owned popups.
        self.surfaces
            .visible_order
            .iter()
            .copied()
            .filter(|key| matches!(key, SurfaceKey::Popup(_)))
            .filter_map(|key| self.surfaces.surface(key).map(|surface| surface.rect))
            .collect()
    }

    /// Returns each relational menu trigger rectangle in declaration order for tests.
    #[cfg(test)]
    pub(crate) fn debug_menu_anchor_rects(&self, root: RootId) -> Option<Vec<Option<Recti>>> {
        self.surfaces.root_node(root)?.root()?.menu_bar.as_ref()?;
        Some(
            self.surfaces
                .nodes
                .iter()
                .filter(|node| self.surfaces.owning_root(node.key) == Some(root))
                .filter_map(|node| {
                    let PopupState::Menu { trigger_slot } = node.popup()? else { return None };
                    Some(node.parent.and_then(|parent| self.menu_surface(parent)?.slot_rect(*trigger_slot)))
                })
                .collect(),
        )
    }

    /// Returns the full committed root-owned menu-bar allocation for layout tests.
    #[cfg(test)]
    pub(crate) fn debug_menu_bar_rect(&self, root: RootId) -> Option<Recti> {
        self.surfaces.root_node(root)?.root()?.menu_bar.as_ref().map(MenuSurface::surface_rect)
    }

    /// Returns active compact menu row rectangles in parent-to-child popup order for tests.
    #[cfg(test)]
    pub(crate) fn debug_active_menu_row_rects(&self) -> Vec<Vec<Recti>> {
        // Active forest order is already parent-first; copy geometry directly from menu bodies.
        self.surfaces
            .visible_order
            .iter()
            .copied()
            .filter_map(|key| {
                let SurfaceKey::Popup(popup_id) = key else { return None };
                let popup = self.surfaces.popup_node(popup_id)?;
                let PopupState::Menu { .. } = popup.popup()? else { return None };
                Some(popup.surface.body.menu()?.slot_rects())
            })
            .collect()
    }
}
