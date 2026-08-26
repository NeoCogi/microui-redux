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

use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::menu::{CompiledMenuPopup, MenuPress, MenuSurface};
use crate::{MouseButton, Node, RootHandle, UiInputEvent, Vec2i, rect};

use super::root_chrome::{RootChromeGeometry, RootChromePart, RootInteraction, record_root_background, record_root_overlay, root_chrome_geometry, root_handle};

/// Failure reported by a checked window or popup mutation.
///
/// Window chrome and popup definitions are owned directly by the window manager, so failures describe
/// only stale identity or explicit ownership and layer policy. No application borrow can make one of
/// these mutations partially succeed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootMutationError {
    /// The supplied [`RootId`] does not identify a retained window or dialog.
    UnknownRoot,
    /// The supplied [`PopupHandle`] no longer identifies a window-owned popup definition.
    UnknownPopup,
    /// A dialog owner is stale, hidden when shown, or is not an ordinary window.
    InvalidRootParent,
    /// The requested fixed layer is outside the supported inclusive range `0..=15`.
    InvalidLayer(u8),
    /// The requested window occupies the manager-controlled modal layer.
    ManagedLayer,
    /// A popup owner is hidden, blocked by a modal, or missing from the active parent path.
    InvalidPopupParent,
}

/// Stable identifier for one popup definition inside its owning window.
///
/// The identifier is intentionally private. Application code proves popup identity by carrying a
/// [`PopupHandle`], so popup definitions cannot be passed to generic window APIs.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct PopupId(usize);

/// Cloneable non-owning capability for one popup retained inside a window.
///
/// A popup has no independent screen root, layer, visibility flag, or destruction operation. Its
/// owner window retains the application tree, while the window manager derives visibility from the
/// single active popup path. Dropping this handle never closes or destroys the popup.
#[derive(Clone)]
pub struct PopupHandle {
    /// Stable popup identity resolved directly in the concrete surface forest.
    id: PopupId,
    /// Weak endpoint for policy-driven dismissal notifications.
    submitted: crate::WidgetEventPortHandle<crate::RootSubmitted>,
}

impl PopupHandle {
    /// Creates the weak capability returned for one newly retained popup.
    fn new(id: PopupId, submitted: crate::WidgetEventPortHandle<crate::RootSubmitted>) -> Self {
        // Construction stays private so callers cannot forge an internal popup identity.
        Self { id, submitted }
    }

    /// Returns whether the owning window still retains this popup definition.
    pub fn is_alive(&self) -> bool {
        // The event owner is stored in the popup record and expires when that record is dropped.
        self.submitted.is_alive()
    }

    /// Returns the weak endpoint emitted when popup policy removes this popup from the active path.
    pub fn submitted(&self) -> crate::WidgetEventPortHandle<crate::RootSubmitted> {
        // Cloning the weak endpoint does not retain the popup tree or owner window.
        self.submitted.clone()
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
    /// Concrete content body with no erased node or dynamic downcast.
    body: SurfaceBody,
}

/// Concrete content variants owned by a forest surface.
enum SurfaceBody {
    /// Ordinary application-authored retained widget tree.
    Widgets(WidgetTree),
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
            body: SurfaceBody::Widgets(WidgetTree::new(content)),
        }
    }

    /// Creates an auto-sized popup around one concrete menu surface.
    fn menu(surface: MenuSurface) -> Self {
        // Menu popups need neither diagnostic title allocation nor generic popup chrome.
        Self {
            name: String::new(),
            options: WindowOption::NO_TITLE | WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE,
            rect: Recti::default(),
            geometry: RootChromeGeometry::default(),
            body: SurfaceBody::Menu(surface),
        }
    }

    /// Measures automatic axes and lays out the application tree in the derived body.
    fn layout(&mut self, menu_bar: Option<&mut MenuSurface>, style: &Style, atlas: &crate::AtlasHandle, viewport: Recti) {
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
                viewport,
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
        self.body.layout(style, atlas, body, viewport);
    }

    /// Returns whether the outer surface contains one screen-space point.
    fn contains(&self, point: Vec2i) -> bool {
        // Root and popup hit testing both begin with the same positive-area rectangle predicate.
        self.rect.contains(&point)
    }
}

impl SurfaceBody {
    /// Measures either concrete body without type erasure.
    fn measure(&mut self, style: &Style, atlas: &crate::AtlasHandle, constraints: crate::Constraints) -> Dimensioni {
        match self {
            Self::Widgets(tree) => tree.measure(style, atlas, constraints),
            Self::Menu(menu) => menu.measure(style, atlas),
        }
    }

    /// Commits one body allocation through the concrete variant.
    fn layout(&mut self, style: &Style, atlas: &crate::AtlasHandle, rect: Recti, viewport: Recti) {
        match self {
            Self::Widgets(tree) => tree.layout(style, atlas.clone(), rect, viewport),
            Self::Menu(menu) => menu.layout(rect, viewport),
        }
    }

    /// Returns whether this body owns pointer capture.
    fn has_capture(&self) -> bool {
        match self {
            Self::Widgets(tree) => tree.has_capture(),
            Self::Menu(menu) => menu.has_capture(),
        }
    }

    /// Clears pointer state while preserving application keyboard focus where applicable.
    fn clear_pointer_targets(&mut self) {
        match self {
            Self::Widgets(tree) => tree.clear_pointer_targets(),
            Self::Menu(menu) => menu.clear_pointer_targets(),
        }
    }

    /// Clears every transient input identity through the concrete body variant.
    fn clear_transient_targets(&mut self) {
        match self {
            Self::Widgets(tree) => tree.clear_transient_targets(),
            Self::Menu(menu) => menu.clear_pointer_targets(),
        }
    }

    /// Returns the concrete menu body when this is a private menu popup.
    fn menu(&self) -> Option<&MenuSurface> {
        match self {
            Self::Menu(menu) => Some(menu),
            Self::Widgets(_) => None,
        }
    }

    /// Returns the mutable concrete menu body when this is a private menu popup.
    fn menu_mut(&mut self) -> Option<&mut MenuSurface> {
        match self {
            Self::Menu(menu) => Some(menu),
            Self::Widgets(_) => None,
        }
    }

    /// Returns the retained widget tree for application-authored bodies only.
    #[cfg(test)]
    fn widgets(&self) -> Option<&WidgetTree> {
        match self {
            Self::Widgets(tree) => Some(tree),
            Self::Menu(_) => None,
        }
    }

    /// Starts an update cycle for widget bodies; concrete menus have no traversal runtime.
    fn begin_update(&mut self) {
        if let Self::Widgets(tree) = self {
            tree.begin_update();
        }
    }

    /// Stages event-local routing only for a retained widget tree.
    fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        if let Self::Widgets(tree) = self {
            tree.begin_input_event(pointer_input_enabled, event);
        }
    }

    /// Routes keyboard/text input only to application widget bodies.
    fn route_focus(&mut self, style: &Style, event: &UiInputEvent) {
        if let Self::Widgets(tree) = self {
            tree.route_focus(style, event);
        }
    }

    /// Updates widget bodies; menu state is committed synchronously by direct pointer routing.
    fn update(&mut self, style: &Style, atlas: crate::AtlasHandle, input: crate::input::InputSnapshot) {
        if let Self::Widgets(tree) = self {
            tree.update(style, atlas, input);
        }
    }

    /// Paints one concrete body into the shared manager display list.
    fn paint(&mut self, display_list: &mut crate::render::DisplayList, style: &Style, atlas: &crate::AtlasHandle) {
        match self {
            Self::Widgets(tree) => tree.paint(display_list, style, atlas.clone()),
            Self::Menu(menu) => menu.paint(display_list, style, atlas),
        }
    }
}

/// One persistent retained application tree and its traversal-local runtime.
struct WidgetTree {
    /// Sole application-authored root node.
    root: Node,
    /// Measurement, layout, input, and paint state associated with `root`.
    runtime: UiRuntime,
}

impl WidgetTree {
    /// Creates traversal state for one newly owned application node.
    fn new(root: Node) -> Self {
        // UiRuntime owns no nodes; this record keeps the parallel values together by construction.
        Self { root, runtime: UiRuntime::new() }
    }

    /// Starts a retained update cycle while preserving focus and capture.
    fn begin_update(&mut self) {
        self.runtime.begin_update();
    }

    /// Clears focus, hover, capture, and staged input without dropping retained widgets.
    fn clear_transient_targets(&mut self) {
        self.runtime.clear_transient_targets();
    }

    /// Revokes only pointer capture, preserving the application's keyboard focus identity.
    fn clear_pointer_targets(&mut self) {
        // Menus are pointer-only manager surfaces. Replacing an application drag must not make that
        // separate interaction authority behave like a focusable retained widget.
        self.runtime.clear_pointer_capture();
    }

    /// Measures the complete application tree under body-space constraints.
    fn measure(&mut self, style: &Style, atlas: &crate::AtlasHandle, constraints: crate::Constraints) -> Dimensioni {
        self.runtime.measure_tree_root(&mut self.root, style, atlas, constraints)
    }

    /// Commits the complete application tree to one screen-space body rectangle.
    fn layout(&mut self, style: &Style, atlas: crate::AtlasHandle, rect: Recti, viewport: Recti) {
        self.runtime.layout_tree_root(&mut self.root, style, atlas, rect, viewport);
    }

    /// Returns whether an application node owns pointer capture.
    fn has_capture(&self) -> bool {
        self.runtime.has_pointer_capture()
    }

    /// Prepares event-local routing for this tree.
    fn begin_input_event(&mut self, pointer_input_enabled: bool, event: &UiInputEvent) {
        self.runtime.begin_input_event(pointer_input_enabled, event);
    }

    /// Routes drag continuation or release directly to the captured application node.
    fn route_captured_pointer(&mut self, style: &Style, mouse_buttons: MouseButton, event: &UiInputEvent) -> Option<bool> {
        self.runtime
            .route_captured_pointer_input_event(std::slice::from_mut(&mut self.root), style, mouse_buttons, event)
    }

    /// Returns whether this runtime accepts an ordinary pointer hit for the current event.
    fn accepts_pointer_input(&self) -> bool {
        self.runtime.accepts_pointer_input()
    }

    /// Routes one ordinary hit and returns the selected retained node identity.
    fn route_pointer(&mut self, style: &Style, event: &UiInputEvent, mouse_buttons: MouseButton) -> Option<crate::ui_node::RuntimeNodeId> {
        // Capture changes only after the selected target and its ancestors classify the event.
        let (owner, result) = self.runtime.route_input_event_to_node_ref(&mut self.root, style, event)?;
        self.runtime.update_pointer_capture(owner, result, event, mouse_buttons);
        Some(owner)
    }

    /// Routes keyboard or text input to this tree's current focus owner.
    fn route_focus(&mut self, style: &Style, event: &UiInputEvent) {
        self.runtime.route_focus_input_event(std::slice::from_mut(&mut self.root), style, event);
    }

    /// Updates every participating application node after event routing.
    fn update(&mut self, style: &Style, atlas: crate::AtlasHandle, input: crate::input::InputSnapshot) {
        self.runtime.update_tree_root(&mut self.root, style, atlas, input);
    }

    /// Records the application tree into the manager display list.
    fn paint(&mut self, display_list: &mut crate::render::DisplayList, style: &Style, atlas: crate::AtlasHandle) {
        self.runtime.paint_tree_root(&mut self.root, display_list, style, atlas);
    }

    /// Resolves one retained node rectangle for relational popup anchors and tests.
    #[cfg(test)]
    fn node_rect(&self, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        // Runtime identities are process-unique, so the first recursive match is authoritative.
        self.runtime.node_rect(std::slice::from_ref(&self.root), node)
    }

    /// Returns committed content size for retained layout tests.
    #[cfg(test)]
    fn content_size(&self) -> Dimensioni {
        self.runtime.debug_root_content_size()
    }

    /// Returns traversal counters for phase-order tests.
    #[cfg(test)]
    fn metrics(&self) -> crate::ui_node::RuntimeMetrics {
        self.runtime.debug_metrics()
    }

    /// Counts application-authored nodes without manager chrome.
    #[cfg(test)]
    fn node_count(&self) -> usize {
        self.root.debug_node_count()
    }
}

/// Private identity for one node in the concrete surface forest.
///
/// Public code continues to carry distinct [`RootId`] and [`PopupHandle`] capabilities. Internally,
/// this compact enum is sufficient for common layout, input, and paint traversal without an erased
/// payload, trait object, or `(owner, popup)` adapter pair.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SurfaceKey {
    /// An ordinary window or modal dialog.
    Root(RootId),
    /// A window- or popup-owned transient surface.
    Popup(PopupId),
}

/// Layer policy stored only on root nodes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RootMode {
    /// Ordinary application window in one caller-selected fixed layer.
    Normal {
        /// Numeric stacking layer in the public fixed range.
        layer: u8,
    },
    /// Dialog in the manager-owned modal layer.
    ///
    /// Its ordinary owner is represented by [`SurfaceNode::parent`], not duplicated here.
    Modal,
}

/// Root-specific policy attached to a concrete surface node.
struct RootState {
    /// Ordinary fixed-layer or manager-owned modal policy.
    mode: RootMode,
    /// Whether layout, input eligibility, and paint currently include this root.
    visible: bool,
    /// Manager-owned title movement or resize gesture.
    interaction: RootInteraction,
    /// Strong owner of user-driven geometry-change events.
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootChanged>>>,
    /// Strong owner of window close submissions.
    submitted_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootSubmitted>>>,
    /// Optional concrete menu bar laid out above the application widget body.
    menu_bar: Option<MenuSurface>,
}

/// Popup-specific policy attached to a concrete surface node.
enum PopupState {
    /// Application-authored popup with exact placement and an observable dismissal event.
    Application {
        /// Exact screen rectangle supplied by the public popup API.
        anchor: Recti,
        /// Strong owner of policy-driven dismissal events.
        submitted_event: Rc<RefCell<crate::event::WidgetEventPort<crate::RootSubmitted>>>,
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
/// Normal roots have no parent, dialogs point to their ordinary owner, top-level popups point to a
/// root, and submenu popups point to their parent popup. This is the only ownership relation stored
/// by the manager; the owning root of any descendant is derived by following it.
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
        match &mut self.surface.body {
            SurfaceBody::Widgets(tree) => tree.clear_transient_targets(),
            SurfaceBody::Menu(menu) => menu.clear_pointer_targets(),
        }
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
        self.surface.geometry.hit_test(point)
    }

    /// Emits the authoritative root geometry after a user move or resize.
    fn emit_changed(&mut self) {
        let event = &self.root().expect("geometry changes are emitted only by roots").changed_event;
        event.borrow_mut().emit(crate::RootChanged { rect: self.surface.rect });
    }

    /// Emits one semantic root submission at the next safe application dispatch boundary.
    fn emit_submitted(&mut self, event: crate::RootSubmitted) {
        self.root()
            .expect("root submissions are emitted only by roots")
            .submitted_event
            .borrow_mut()
            .emit(event);
    }

    /// Dismisses an active popup and emits exactly one policy event.
    fn dismiss_popup(&mut self) {
        // The caller walks the active leaf upward, so descendants are always notified first.
        self.surface.body.clear_transient_targets();
        if let Some(PopupState::Application { submitted_event, .. }) = self.popup() {
            // Menu popups have no unobservable lifecycle port; their visibility is wholly internal.
            submitted_event.borrow_mut().emit(crate::RootSubmitted::PopupDismissed);
        }
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
/// parent edges. `visible_order` is the reusable layered traversal consumed by layout, input, paint,
/// and diagnostics.
pub(super) struct SurfaceForest {
    /// Retained surface storage and authoritative chronological order for root nodes.
    ///
    /// Indices are deliberately not exposed as identity because fronting a root moves its complete
    /// node to this vector's tail. Stable keys and parent edges remain valid across that move.
    nodes: Vec<SurfaceNode>,
    /// Deepest popup in the sole active branch, or no visible popup branch.
    active_popup: Option<PopupId>,
    /// Materialized back-to-front visible traversal, reused without per-frame allocation.
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
        self.node(SurfaceKey::Root(root)).filter(|node| node.root().is_some())
    }

    /// Mutably borrows one retained root node.
    fn root_node_mut(&mut self, root: RootId) -> Option<&mut SurfaceNode> {
        self.node_mut(SurfaceKey::Root(root)).filter(|node| node.root().is_some())
    }

    /// Borrows one retained popup node.
    fn popup_node(&self, popup: PopupId) -> Option<&SurfaceNode> {
        self.node(SurfaceKey::Popup(popup)).filter(|node| node.popup().is_some())
    }

    /// Mutably borrows one retained popup node.
    fn popup_node_mut(&mut self, popup: PopupId) -> Option<&mut SurfaceNode> {
        self.node_mut(SurfaceKey::Popup(popup)).filter(|node| node.popup().is_some())
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

    /// Returns the newest visible modal root from global activation chronology.
    fn active_modal_root(&self) -> Option<RootId> {
        self.nodes.iter().rev().find_map(|node| match (node.key, node.root()) {
            (SurfaceKey::Root(root), Some(state)) if state.mode == RootMode::Modal && state.visible => Some(root),
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
        let active_mode = active_owner.and_then(|root| self.root_node(root)?.root().map(|state| state.mode));

        let mut popup_path = std::mem::take(&mut self.popup_path_scratch);
        self.fill_active_popup_path(&mut popup_path);
        let mut visible = std::mem::take(&mut self.visible_order);
        visible.clear();

        // Sixteen fixed scans keep layer grouping explicit and allocation-free. Each scan preserves
        // the relative order of root nodes in `nodes`; interleaved popup nodes are ignored.
        for layer in MIN_LAYER..=MAX_LAYER {
            for node in &self.nodes {
                if let (SurfaceKey::Root(root), Some(state)) = (node.key, node.root())
                    && state.mode == (RootMode::Normal { layer })
                    && state.visible
                {
                    visible.push(SurfaceKey::Root(root));
                }
            }
            if active_mode == Some(RootMode::Normal { layer }) {
                visible.extend_from_slice(&popup_path);
            }
        }
        // Modal roots form the final structural tier while retaining the same global chronology.
        for node in &self.nodes {
            if let (SurfaceKey::Root(root), Some(state)) = (node.key, node.root())
                && state.mode == RootMode::Modal
                && state.visible
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

    /// Returns the shared back-to-front traversal used by every visible-surface phase.
    fn visible_surfaces(&self) -> &[SurfaceKey] {
        &self.visible_order
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
    /// Borrows one mounted menu item's concrete public state through its typed capability.
    pub(crate) fn menu_item(&self, handle: &crate::MenuItemHandle) -> Result<&crate::MenuItemParameters, crate::MenuItemAccessError> {
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

    /// Mutably borrows one mounted menu item's concrete public state through its typed capability.
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
                (SurfaceKind::Popup(_), SurfaceBody::Widgets(_)) => {}
                (SurfaceKind::Popup(_), SurfaceBody::Menu(_)) => {
                    unreachable!("surface role and concrete body must remain structurally aligned")
                }
            }
        }
        unreachable!("a validated menu item must remain mounted during one exclusive manager borrow")
    }

    /// Registers one concrete root after validating its optional dialog parent edge.
    fn register_window(
        &mut self,
        mode: RootMode,
        parent: Option<RootId>,
        window: Window,
        options: WindowOption,
        visible: bool,
    ) -> Result<RootHandle, RootMutationError> {
        if mode == RootMode::Modal {
            let owner = parent.ok_or(RootMutationError::InvalidRootParent)?;
            let owner = self.root_node(owner)?.root().expect("root lookup must return root policy");
            if !matches!(owner.mode, RootMode::Normal { .. }) {
                return Err(RootMutationError::InvalidRootParent);
            }
        } else {
            debug_assert!(parent.is_none(), "ordinary roots cannot have a structural parent");
        }

        let (name, rect, content, menu_bar) = window.into_parts();
        // Compile before mounting the root, retaining recursive popup ownership until each direct
        // forest parent has been inserted. No numeric menu namespace or topology translation exists.
        let (menu_bar, menu_popups) = match menu_bar.map(crate::MenuBar::compile) {
            Some(menu) => (Some(menu.bar), menu.popups),
            None => (None, Vec::new()),
        };

        // Strong event owners live with root policy; returned handles retain neither them nor trees.
        let changed_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let changed = crate::WidgetEventPortHandle::new(&changed_event);
        let submitted = crate::WidgetEventPortHandle::new(&submitted_event);
        let id = self.next_root_id();
        self.surfaces.insert_root(SurfaceNode {
            key: SurfaceKey::Root(id),
            parent: parent.map(SurfaceKey::Root),
            surface: Surface::new(name, options, rect, content),
            kind: SurfaceKind::Root(RootState {
                mode,
                visible,
                interaction: RootInteraction::None,
                changed_event,
                submitted_event,
                menu_bar,
            }),
        });

        // Each recursive transport value becomes one direct child edge. Branch labels remain in the
        // parent surface, so release builds allocate no separate diagnostic popup names.
        for popup in menu_popups {
            self.register_menu_popup(SurfaceKey::Root(id), popup);
        }
        self.surfaces.rebuild_visible_order();
        self.invalidate_ui_commit();
        Ok(root_handle(id, changed, submitted))
    }

    /// Allocates a root identifier that will never be reused.
    fn next_root_id(&mut self) -> RootId {
        let id = RootId::from_raw(self.next_root_id);
        self.next_root_id = self.next_root_id.checked_add(1).expect("retained window id counter overflowed");
        id
    }

    /// Allocates a popup identifier that will never be reused.
    fn next_popup_id(&mut self) -> PopupId {
        let id = PopupId(self.next_popup_id);
        self.next_popup_id = self.next_popup_id.checked_add(1).expect("retained popup id counter overflowed");
        id
    }

    /// Creates an open ordinary window from one complete retained definition.
    pub fn create_window(&mut self, window: Window) -> RootHandle {
        // Ordinary construction has no fallible owner edge.
        self.register_window(RootMode::Normal { layer: DEFAULT_LAYER }, None, window, WindowOption::FRAME, true)
            .expect("ordinary window registration cannot fail")
    }

    /// Creates a hidden modal dialog directly owned by an ordinary window.
    pub fn create_dialog(&mut self, owner: RootId, window: Window) -> Result<RootHandle, RootMutationError> {
        // Dialog validation and optional menu compilation share ordinary window registration.
        self.register_window(RootMode::Modal, Some(owner), window, WindowOption::FRAME, false)
    }

    /// Creates a hidden top-level popup definition inside one window or dialog.
    pub fn create_popup(&mut self, owner: RootId, name: &str, content: Node) -> Result<PopupHandle, RootMutationError> {
        self.root_node(owner)?;
        // A top-level popup's sole parent edge points directly to its owning root.
        Ok(self.register_application_popup(SurfaceKey::Root(owner), name, content))
    }

    /// Registers one application-authored popup below an already validated root.
    fn register_application_popup(&mut self, parent: SurfaceKey, name: &str, content: Node) -> PopupHandle {
        let id = self.next_popup_id();
        let submitted_event = Rc::new(RefCell::new(crate::event::WidgetEventPort::new()));
        let submitted = crate::WidgetEventPortHandle::new(&submitted_event);
        self.surfaces.insert_popup(SurfaceNode {
            key: SurfaceKey::Popup(id),
            parent: Some(parent),
            surface: Surface::new(name.to_owned(), Self::default_popup_options(), Recti::default(), content),
            kind: SurfaceKind::Popup(PopupState::Application {
                anchor: Recti::default(),
                submitted_event,
            }),
        });
        self.invalidate_ui_commit();
        PopupHandle::new(id, submitted)
    }

    /// Recursively inserts one concrete menu popup beneath its direct forest parent.
    fn register_menu_popup(&mut self, parent: SurfaceKey, popup: CompiledMenuPopup) {
        let CompiledMenuPopup { trigger_slot, surface, children } = popup;
        let id = self.next_popup_id();
        self.surfaces.insert_popup(SurfaceNode {
            key: SurfaceKey::Popup(id),
            parent: Some(parent),
            surface: Surface::menu(surface),
            kind: SurfaceKind::Popup(PopupState::Menu { trigger_slot }),
        });
        // The parent is retained before recursion, satisfying the forest's sole structural invariant.
        for child in children {
            self.register_menu_popup(SurfaceKey::Popup(id), child);
        }
    }

    /// Replaces a retained window title silently.
    pub fn set_root_name(&mut self, root: RootId, name: String) -> Result<(), RootMutationError> {
        self.root_node_mut(root)?.surface.name = name;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a window outer rectangle silently.
    pub fn set_root_rect(&mut self, root: RootId, rect: Recti) -> Result<(), RootMutationError> {
        self.root_node_mut(root)?.surface.rect = rect;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces a window outer size without changing its screen origin.
    pub fn set_root_size(&mut self, root: RootId, size: Dimensioni) -> Result<(), RootMutationError> {
        let surface = &mut self.root_node_mut(root)?.surface;
        surface.rect.width = size.width;
        surface.rect.height = size.height;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces window chrome options and reconciles capture immediately.
    pub fn set_root_options(&mut self, root: RootId, options: WindowOption) -> Result<(), RootMutationError> {
        self.root_node_mut(root)?.set_root_options(options);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Replaces popup frame, padding, and automatic-size policy through its typed handle.
    pub fn set_popup_options(&mut self, popup: &PopupHandle, options: WindowOption) -> Result<(), RootMutationError> {
        let node = self.popup_node_mut(popup)?;
        // Popups never acquire manager chrome; enforce that invariant regardless of caller flags.
        node.surface.options = options | WindowOption::NO_TITLE | WindowOption::NO_RESIZE;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Assigns an ordinary window to one fixed application stacking layer.
    pub fn set_root_layer(&mut self, root: RootId, layer: u8) -> Result<(), RootMutationError> {
        if layer > MAX_LAYER {
            return Err(RootMutationError::InvalidLayer(layer));
        }
        let state = self.root_node(root)?.root().expect("root lookup must return root policy");
        if !matches!(state.mode, RootMode::Normal { .. }) {
            return Err(RootMutationError::ManagedLayer);
        }
        self.surfaces.set_root_layer(root, layer);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Returns the fixed or modal layer policy of one window.
    pub fn root_layer_binding(&self, root: RootId) -> Result<LayerBinding, RootMutationError> {
        let state = self.root_node(root)?.root().expect("root lookup must return root policy");
        Ok(match state.mode {
            RootMode::Normal { layer } => LayerBinding::Fixed(layer),
            RootMode::Modal => LayerBinding::Modal,
        })
    }

    /// Shows or hides a window while retaining its application and popup definitions.
    pub fn set_root_visible(&mut self, root: RootId, visible: bool) -> Result<(), RootMutationError> {
        let mode = self.root_node(root)?.root().expect("root lookup must return root policy").mode;
        if visible {
            if mode == RootMode::Modal {
                let owner = match self.root_node(root)?.parent {
                    Some(SurfaceKey::Root(owner)) => owner,
                    _ => return Err(RootMutationError::InvalidRootParent),
                };
                if !self.root_is_visible(owner) {
                    return Err(RootMutationError::InvalidRootParent);
                }
                if self.active_modal_root() != Some(root) {
                    self.dismiss_active_popups();
                }
            }
            self.root_node_mut(root)?.set_root_visible(true);
            self.raise_root(root);
        } else {
            // The one parent edge makes directly owned dialogs discoverable without a second map.
            let affected = self.owned_root_ids(root);
            if self.active_popup_owner().is_some_and(|owner| affected.contains(&owner)) {
                self.dismiss_active_popups();
            }
            for affected_root in affected {
                self.root_node_mut(affected_root)
                    .expect("collected root must remain retained")
                    .set_root_visible(false);
            }
            if self.active_root.is_some_and(|active| active == root) {
                self.active_root = None;
            }
            self.surfaces.rebuild_visible_order();
        }
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Shows one popup at the current pointer position.
    pub fn show_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        let mouse = self.input.snapshot().mouse_pos;
        self.show_popup_at(popup, rect(mouse.x, mouse.y, 1, 1))
    }

    /// Shows one popup at an exact screen-space anchor and updates the sole active path.
    pub fn show_popup_at(&mut self, popup: &PopupHandle, anchor: Recti) -> Result<(), RootMutationError> {
        // Open first so an invalid child path cannot partially replace its retained anchor.
        self.open_popup_id(popup.id)?;
        let node = self.popup_node_mut(popup)?;
        let PopupState::Application { anchor: current, .. } = node.popup_mut().expect("popup lookup must return popup policy") else {
            return Err(RootMutationError::UnknownPopup);
        };
        *current = anchor;
        node.surface.rect = anchor;
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Opens one retained popup identity without replacing its configured placement relation.
    fn open_popup_id(&mut self, popup: PopupId) -> Result<(), RootMutationError> {
        let node = self.surfaces.popup_node(popup).ok_or(RootMutationError::UnknownPopup)?;
        let parent = node.parent.expect("every popup must retain one parent edge");
        let owner = self.surfaces.owning_root(SurfaceKey::Popup(popup)).ok_or(RootMutationError::UnknownPopup)?;
        if !self.popup_owner_is_eligible(owner) {
            return Err(RootMutationError::InvalidPopupParent);
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
                    return Err(RootMutationError::InvalidPopupParent);
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
    pub fn hide_popup(&mut self, popup: &PopupHandle) -> Result<(), RootMutationError> {
        self.popup_node(popup)?;
        if self.surfaces.popup_is_active(popup.id) {
            let depth = self.surfaces.popup_depth(popup.id).expect("active popup must have rooted ancestry");
            self.truncate_active_popup_path(depth);
        }
        Ok(())
    }

    /// Raises one root inside its incrementally maintained structural layer.
    pub fn bring_root_to_front(&mut self, root: RootId) -> Result<(), RootMutationError> {
        let state = self.root_node(root)?.root().expect("root lookup must return root policy");
        if state.mode == RootMode::Modal && state.visible && self.active_modal_root() != Some(root) {
            self.dismiss_active_popups();
        }
        self.raise_root(root);
        self.invalidate_ui_commit();
        Ok(())
    }

    /// Destroys one window and, for an ordinary window, its directly owned dialogs.
    pub fn destroy_root(&mut self, root: RootId) -> bool {
        if self.root_node(root).is_err() {
            return false;
        }
        let removed = self.owned_root_ids(root);
        if self.active_popup_owner().is_some_and(|owner| removed.contains(&owner)) {
            self.dismiss_active_popups();
        }
        if self.active_root.is_some_and(|active| removed.contains(&active)) {
            self.active_root = None;
        }
        self.surfaces.remove_roots(&removed);
        self.invalidate_ui_commit();
        true
    }

    /// Resolves a public root identity directly to its concrete forest node.
    fn root_node(&self, root: RootId) -> Result<&SurfaceNode, RootMutationError> {
        self.surfaces.root_node(root).ok_or(RootMutationError::UnknownRoot)
    }

    /// Mutably resolves a public root identity to its concrete forest node.
    fn root_node_mut(&mut self, root: RootId) -> Result<&mut SurfaceNode, RootMutationError> {
        self.surfaces.root_node_mut(root).ok_or(RootMutationError::UnknownRoot)
    }

    /// Resolves a typed popup capability directly to its concrete forest node.
    fn popup_node(&self, popup: &PopupHandle) -> Result<&SurfaceNode, RootMutationError> {
        self.surfaces.popup_node(popup.id).ok_or(RootMutationError::UnknownPopup)
    }

    /// Mutably resolves a typed popup capability directly to its concrete forest node.
    fn popup_node_mut(&mut self, popup: &PopupHandle) -> Result<&mut SurfaceNode, RootMutationError> {
        self.surfaces.popup_node_mut(popup.id).ok_or(RootMutationError::UnknownPopup)
    }

    /// Collects one root and the modal roots whose sole parent edge points directly to it.
    fn owned_root_ids(&self, root: RootId) -> Vec<RootId> {
        let mut ids = vec![root];
        if self
            .root_node(root)
            .ok()
            .and_then(SurfaceNode::root)
            .is_some_and(|state| matches!(state.mode, RootMode::Normal { .. }))
        {
            ids.extend(self.surfaces.nodes.iter().filter_map(|node| match (node.key, node.parent, node.root()) {
                (SurfaceKey::Root(dialog), Some(SurfaceKey::Root(owner)), Some(state)) if owner == root && state.mode == RootMode::Modal => Some(dialog),
                _ => None,
            }));
        }
        ids
    }

    /// Returns whether a popup owner may participate under current visibility and modal policy.
    fn popup_owner_is_eligible(&self, owner: RootId) -> bool {
        let Some(state) = self.surfaces.root_node(owner).and_then(SurfaceNode::root) else {
            return false;
        };
        if !state.visible {
            return false;
        }
        match self.active_modal_root() {
            Some(modal) => owner == modal,
            None => matches!(state.mode, RootMode::Normal { .. }),
        }
    }

    /// Returns the owning root derived from the deepest active popup's ancestry.
    fn active_popup_owner(&self) -> Option<RootId> {
        self.surfaces.active_popup.and_then(|popup| self.surfaces.owning_root(SurfaceKey::Popup(popup)))
    }

    /// Removes the active path suffix after `keep`, notifying deepest popups first.
    fn truncate_active_popup_path(&mut self, keep: usize) {
        let Some(mut current) = self.surfaces.active_popup else { return };
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

    /// Makes one retained root the newest entry in global activation chronology.
    fn raise_root(&mut self, root: RootId) {
        // The forest moves the complete node so layer changes can later reuse this chronology
        // without a parallel sequence number or order index.
        self.surfaces.move_root_to_front(root);
    }

    /// Returns the frontmost visible modal dialog.
    fn active_modal_root(&self) -> Option<RootId> {
        self.surfaces.active_modal_root()
    }

    /// Returns an immutable common surface by concrete traversal key.
    fn surface(&self, key: SurfaceKey) -> Option<&Surface> {
        self.surfaces.surface(key)
    }

    /// Returns a mutable common surface by concrete traversal key.
    fn surface_mut(&mut self, key: SurfaceKey) -> Option<&mut Surface> {
        self.surfaces.surface_mut(key)
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
                .is_some_and(|root| matches!(root.mode, RootMode::Normal { .. })),
        }
    }

    /// Returns whether a surface owns manager or application pointer capture.
    fn surface_has_capture(&self, key: SurfaceKey) -> bool {
        self.surfaces.node(key).is_some_and(SurfaceNode::has_capture)
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
        for index in 0..self.surfaces.visible_surfaces().len() {
            let key = self.surfaces.visible_surfaces()[index];
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

    /// Clears capture and focus from every surface except `keep`.
    fn clear_other_captures(&mut self, keep: SurfaceKey) {
        // Global capture policy permits at most one concrete surface to retain transient ownership.
        for node in &mut self.surfaces.nodes {
            if node.key != keep && node.has_capture() {
                node.clear_transient_targets();
            }
        }
    }

    /// Revokes competing pointer gestures without clearing any application keyboard focus.
    fn clear_other_pointer_captures(&mut self, keep: SurfaceKey) {
        for node in &mut self.surfaces.nodes {
            if node.key != keep {
                node.surface.body.clear_pointer_targets();
                if let Some(root) = node.root_mut()
                    && let Some(menu) = root.menu_bar.as_mut()
                {
                    menu.clear_pointer_targets();
                }
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

    /// Applies one concrete menu press after releasing the surface borrow that produced it.
    fn apply_menu_press(&mut self, surface: SurfaceKey, press: MenuPress, previous_top: Option<PopupId>) {
        match press {
            MenuPress::OpenSlot(slot) => {
                let target = self
                    .surfaces
                    .nodes
                    .iter()
                    .find(|node| node.parent == Some(surface) && matches!(node.popup(), Some(PopupState::Menu { trigger_slot }) if *trigger_slot == slot))
                    .and_then(|node| match node.key {
                        SurfaceKey::Popup(popup) => Some(popup),
                        SurfaceKey::Root(_) => None,
                    })
                    .expect("a menu branch slot must retain one direct popup child");
                // Outside dismissal already closed the old path before this bar press reached the
                // window. A repeated heading press therefore toggles closed instead of reopening.
                if matches!(surface, SurfaceKey::Root(_)) && previous_top == Some(target) {
                    return;
                }
                self.open_popup_id(target).expect("a staged menu action must retain an eligible declared popup");
            }
            MenuPress::Close => {
                // The surface already queued the typed event. Arm tail suppression before removing
                // its popup so the physical release cannot fall through to newly exposed content.
                self.discard_pointer_capture_tail = true;
                self.dismiss_active_popups();
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
                        self.set_root_visible(root, false).expect("chrome target must remain registered");
                        self.root_node_mut(root)
                            .expect("closing a root must not destroy it")
                            .emit_submitted(crate::RootSubmitted::Close);
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
                    node.emit_changed();
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

    /// Lays out the single reusable visible traversal in exact back-to-front order.
    fn layout(&mut self, viewport: Recti, atlas: &crate::AtlasHandle) {
        self.sync_menu_presentation();
        self.surfaces.rebuild_visible_order();
        let style = self.style;
        for index in 0..self.surfaces.nodes.len() {
            let key = self.surfaces.nodes[index].key;
            if !self.surfaces.visible_surfaces().contains(&key) {
                self.surfaces.nodes[index].clear_transient_targets();
            }
        }

        // Keys are copied one at a time so mutating geometry never requires cloning the traversal.
        for index in 0..self.surfaces.visible_surfaces().len() {
            let key = self.surfaces.visible_surfaces()[index];
            if matches!(key, SurfaceKey::Popup(_)) {
                let rect = self
                    .resolved_popup_rect(key)
                    .expect("active popup anchor node must remain in its retained parent surface");
                self.surface_mut(key).expect("visible popup must remain retained").rect = rect;
            }
            let node = self.surfaces.node_mut(key).expect("visible surface must remain retained");
            node.layout(&style, atlas, viewport);
        }
    }

    /// Routes and applies one normalized event across every eligible surface.
    fn update_for_event(&mut self, atlas: &crate::AtlasHandle, event: &UiInputEvent, input: crate::input::InputSnapshot) {
        let style = self.style;
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

        let hover = (event.is_pointer() && !discard_pointer)
            .then(|| self.input_surface_at(input.mouse_pos))
            .flatten();
        if matches!(event, UiInputEvent::MouseDown { .. })
            && let Some(surface) = hover
        {
            self.activate_pointer_surface(surface);
            let owner = self.surfaces.owning_root(surface).expect("visible surface must retain a root ancestor");
            self.bring_root_to_front(owner).expect("pointer target owner must remain registered");
            if self.menu_contains(surface, input.mouse_pos) {
                // A pointer-only menu gesture may preempt application capture but never keyboard focus.
                self.clear_other_pointer_captures(surface);
            } else {
                self.clear_other_captures(surface);
            }
        }

        let drag = (!discard_pointer).then(|| self.drag_input_surface()).flatten();
        let pointer = match event {
            UiInputEvent::MouseDrag { .. } => drag,
            _ => hover,
        };
        let keyboard = self.keyboard_input_surface();
        let modal = self.active_modal_root();

        for index in 0..self.surfaces.visible_surfaces().len() {
            let key = self.surfaces.visible_surfaces()[index];
            if self.surface_is_eligible(key, modal) {
                let widget_pointer = pointer == Some(key) && !self.menu_contains(key, input.mouse_pos);
                self.surface_mut(key)
                    .expect("visible input surface must remain retained")
                    .body
                    .begin_input_event(widget_pointer, event);
            }
        }

        let mut staged_menu_press = None;
        if event.is_pointer() && !discard_pointer {
            let captured = matches!(event, UiInputEvent::MouseDrag { .. } | UiInputEvent::MouseUp { .. })
                .then(|| self.captured_input_surface())
                .flatten();
            let mut handled = false;
            if let Some(surface) = captured {
                if self.menu_surface(surface).is_some_and(MenuSurface::has_capture) {
                    let route = self.route_menu_pointer(surface, event).expect("captured menu surface must remain retained");
                    staged_menu_press = route.press.map(|press| (surface, press));
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
                                match &mut self.surface_mut(surface).expect("captured window must remain retained").body {
                                    SurfaceBody::Widgets(tree) => tree.route_captured_pointer(&style, input.mouse_buttons, event).is_some(),
                                    SurfaceBody::Menu(_) => false,
                                }
                            }
                        }
                        SurfaceKey::Popup(_) => match &mut self.surface_mut(surface).expect("captured popup must remain retained").body {
                            SurfaceBody::Widgets(tree) => tree.route_captured_pointer(&style, input.mouse_buttons, event).is_some(),
                            SurfaceBody::Menu(_) => false,
                        },
                    };
                }
            }
            if !handled && let Some(surface) = pointer {
                handled = matches!(surface, SurfaceKey::Root(root) if self.route_chrome_event(root, event));
                if !handled && self.menu_contains(surface, input.mouse_pos) {
                    let route = self.route_menu_pointer(surface, event).expect("hit menu surface must remain retained");
                    staged_menu_press = route.press.map(|press| (surface, press));
                    handled = route.handled;
                }
                if !handled {
                    let body = &mut self.surface_mut(surface).expect("pointer surface must remain retained").body;
                    if let SurfaceBody::Widgets(tree) = body
                        && tree.accepts_pointer_input()
                    {
                        let _ = tree.route_pointer(&style, event, input.mouse_buttons);
                    }
                }
            }
        } else if event.is_focus_input()
            && let Some(surface) = keyboard
        {
            self.surface_mut(surface)
                .expect("keyboard surface must remain retained")
                .body
                .route_focus(&style, event);
        }

        let modal = self.active_modal_root();
        for index in 0..self.surfaces.nodes.len() {
            let key = self.surfaces.nodes[index].key;
            let participates = self.surfaces.visible_surfaces().contains(&key) && self.surface_is_eligible(key, modal);
            if !participates {
                self.surfaces.nodes[index].clear_transient_targets();
            }
        }
        for index in 0..self.surfaces.visible_surfaces().len() {
            let key = self.surfaces.visible_surfaces()[index];
            if self.surface_is_eligible(key, modal) {
                self.surface_mut(key)
                    .expect("visible update surface must remain retained")
                    .body
                    .update(&style, atlas.clone(), input);
            }
        }
        if self.active_root.is_some_and(|root| !self.root_is_visible(root)) {
            self.active_root = None;
        }
        if let Some((surface, press)) = staged_menu_press {
            // Direct routing has ended its concrete surface borrow before forest visibility changes.
            self.apply_menu_press(surface, press, previous_top);
        }
    }

    /// Paints the same reusable visible traversal used by layout and input selection.
    pub(crate) fn paint(&mut self, dimensions: Dimensioni, atlas: &crate::AtlasHandle) {
        self.display_list.clear();
        let viewport = Recti::new(0, 0, dimensions.width, dimensions.height);
        self.surfaces.rebuild_visible_order();
        let style = self.style;
        for index in 0..self.surfaces.visible_surfaces().len() {
            let key = self.surfaces.visible_surfaces()[index];
            let node_index = self.surfaces.node_index(key).expect("visible surface must remain retained");
            let node = &mut self.surfaces.nodes[node_index];
            record_root_background(&mut self.display_list, viewport, node.surface.rect, node.surface.options, &style);
            if let Some(root) = node.root_mut()
                && let Some(bar) = root.menu_bar.as_mut()
            {
                bar.paint(&mut self.display_list, &style, atlas);
            }
            node.surface.body.paint(&mut self.display_list, &style, atlas);
            if matches!(key, SurfaceKey::Root(_)) {
                record_root_overlay(&mut self.display_list, viewport, &node.surface.name, node.surface.geometry, &style, atlas);
            }
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

    /// Records ordinary-window keyboard activation for one pointer surface.
    fn activate_pointer_surface(&mut self, surface: SurfaceKey) {
        let Some(owner) = self.surfaces.owning_root(surface) else { return };
        if self
            .surfaces
            .root_node(owner)
            .and_then(SurfaceNode::root)
            .is_some_and(|state| matches!(state.mode, RootMode::Normal { .. }))
        {
            self.active_root = Some(owner);
        }
    }

    /// Returns whether one retained window is visible.
    fn root_is_visible(&self, root: RootId) -> bool {
        self.surfaces.root_node(root).and_then(SurfaceNode::root).is_some_and(|state| state.visible)
    }

    /// Returns the front eligible surface containing one pointer point.
    fn input_surface_at(&self, point: Vec2i) -> Option<SurfaceKey> {
        let modal = self.active_modal_root();
        self.surfaces
            .visible_surfaces()
            .iter()
            .rev()
            .copied()
            .find(|key| self.surface_is_eligible(*key, modal) && self.surface(*key).is_some_and(|surface| surface.contains(point)))
    }

    /// Returns the front visible surface in the current modal group.
    fn front_input_surface(&self) -> Option<SurfaceKey> {
        let modal = self.active_modal_root();
        self.surfaces
            .visible_surfaces()
            .iter()
            .rev()
            .copied()
            .find(|key| self.surface_is_eligible(*key, modal))
    }

    /// Returns the eligible surface that currently owns pointer capture.
    fn captured_input_surface(&self) -> Option<SurfaceKey> {
        let modal = self.active_modal_root();
        self.surfaces
            .visible_surfaces()
            .iter()
            .rev()
            .copied()
            .find(|key| self.surface_is_eligible(*key, modal) && self.surface_has_capture(*key))
    }

    /// Returns the surface receiving pointer-drag continuation.
    fn drag_input_surface(&self) -> Option<SurfaceKey> {
        self.captured_input_surface().or_else(|| self.front_input_surface())
    }

    /// Returns the sole surface receiving keyboard and text input.
    fn keyboard_input_surface(&self) -> Option<SurfaceKey> {
        if let Some(modal) = self.active_modal_root() {
            return Some(SurfaceKey::Root(modal));
        }
        self.captured_input_surface()
            .or_else(|| self.active_root.filter(|root| self.root_is_visible(*root)).map(SurfaceKey::Root))
            .or_else(|| {
                self.front_input_surface().and_then(|surface| {
                    let owner = self.surfaces.owning_root(surface)?;
                    self.surfaces
                        .root_node(owner)
                        .and_then(SurfaceNode::root)
                        .filter(|state| matches!(state.mode, RootMode::Normal { .. }))
                        .map(|_| SurfaceKey::Root(owner))
                })
            })
    }

    /// Returns the popup options installed for new definitions.
    const fn default_popup_options() -> WindowOption {
        WindowOption::FRAME
            .union(WindowOption::AUTO_SIZE)
            .union(WindowOption::NO_RESIZE)
            .union(WindowOption::NO_TITLE)
    }

    /// Returns visible surface names in exact paint order for tests.
    #[cfg(test)]
    pub(crate) fn debug_rendered_root_names(&self) -> Vec<String> {
        self.surfaces
            .visible_surfaces()
            .iter()
            .filter_map(|key| self.surface(*key).map(|surface| surface.name.clone()))
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

    /// Returns the active ordinary window for tests.
    #[cfg(test)]
    pub(crate) fn debug_active_root(&self) -> Option<RootId> {
        self.active_root
    }

    /// Returns the active modal dialog for tests.
    #[cfg(test)]
    pub(crate) fn debug_modal_root(&self) -> Option<RootId> {
        self.active_modal_root()
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
        self.surfaces.root_node(root)?.surface.body.widgets().map(WidgetTree::metrics)
    }

    /// Returns combined window pointer capture for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_has_pointer_capture(&self, root: RootId) -> Option<bool> {
        self.surfaces.root_node(root).map(SurfaceNode::has_capture)
    }

    /// Counts application nodes in one window for tests.
    #[cfg(test)]
    pub(crate) fn debug_root_node_count(&self, root: RootId) -> Option<usize> {
        self.surfaces.root_node(root)?.surface.body.widgets().map(WidgetTree::node_count)
    }

    /// Returns one application node rectangle inside a window.
    #[cfg(test)]
    pub(crate) fn debug_root_node_rect(&self, root: RootId, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.surfaces.root_node(root)?.surface.body.widgets()?.node_rect(node)
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
        self.popup_node(popup).ok()?;
        Some(self.surfaces.popup_is_active(popup.id))
    }

    /// Returns popup content size through its typed handle for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_content_size(&self, popup: &PopupHandle) -> Option<Dimensioni> {
        Some(self.popup_node(popup).ok()?.surface.body.widgets()?.content_size())
    }

    /// Returns one retained node rectangle inside a popup for tests.
    #[cfg(test)]
    pub(crate) fn debug_popup_node_rect(&self, popup: &PopupHandle, node: crate::ui_node::RuntimeNodeId) -> Option<Recti> {
        self.popup_node(popup).ok()?.surface.body.widgets()?.node_rect(node)
    }

    /// Returns the active popup names in parent-to-child order for path tests.
    #[cfg(test)]
    pub(crate) fn debug_active_popup_names(&self) -> Vec<String> {
        self.surfaces
            .visible_surfaces()
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
            .visible_surfaces()
            .iter()
            .copied()
            .filter(|key| matches!(key, SurfaceKey::Popup(_)))
            .filter_map(|key| self.surface(key).map(|surface| surface.rect))
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
            .visible_surfaces()
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
