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

//! Concrete retained application menus.
//!
//! Applications create each menu item as an ordinary retained widget, register that item's event
//! with the context, and then transfer its node through MenuGroup, Menu, and MenuPanel into one
//! WindowMenu. No command value, generic menu type, menu-specific type erasure, or copied menu
//! specification exists. Live item state stays on the concrete MenuItem and is mutated through its
//! TypedWidgetHandle.

use crate::render::RendererBackend;
use crate::ui_node::widgets::{MenuBar, MenuBarSubmitted, MenuSeparator};
use crate::{Context, EventContext, Linear, LinearItem, LinearParameters, Node, PopupHandle, Recti, RootHandle, RootSubmitted, SubscribeError, TypedWidgetHandle};
pub use crate::ui_node::widgets::{MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted};

/// Accessor retained by context subscriptions to locate one application-owned menu.
type WindowMenuAccessor<State> = for<'a> fn(&'a mut State) -> &'a mut WindowMenu;

/// One concrete separator-delimited group of retained menu item nodes.
pub struct MenuGroup {
    /// Unique unmounted nodes transferred into this group in display order.
    items: Vec<Node>,
}

impl MenuGroup {
    /// Creates a group from concrete retained menu item nodes.
    pub fn new(items: impl IntoIterator<Item = Node>) -> Self {
        // Collect once at the ownership boundary; no description is cloned when a menu opens.
        Self { items: items.into_iter().collect() }
    }

    /// Returns the number of retained item nodes waiting to be mounted.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether this group contains no item nodes.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Transfers this group's unique nodes to its concrete menu.
    fn into_items(self) -> Vec<Node> {
        self.items
    }
}

/// One concrete top-level menu heading and its retained item groups.
pub struct Menu {
    /// User-visible top-level heading.
    label: String,
    /// Concrete groups transferred into this menu in display order.
    groups: Vec<MenuGroup>,
}

impl Menu {
    /// Creates a top-level menu from a label and concrete groups.
    pub fn new(label: impl Into<String>, groups: impl IntoIterator<Item = MenuGroup>) -> Self {
        // Labels remain local to bar construction; groups keep unique ownership of their item nodes.
        Self {
            label: label.into(),
            groups: groups.into_iter().collect(),
        }
    }

    /// Returns the top-level heading label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the number of groups waiting to be mounted.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Returns whether this menu contains no groups.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Mounts all non-empty groups into one concrete vertical popup tree.
    fn into_popup(self) -> (String, Node) {
        let mut rows = Vec::new();
        for group in self.groups {
            let items = group.into_items();
            if items.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                // Group boundaries become ordinary retained separator widgets.
                rows.push(LinearItem::content(MenuSeparator::create()));
            }
            rows.extend(items.into_iter().map(LinearItem::content));
        }

        // Reuse the established retained linear container for row ownership, measurement, and
        // placement. The popup root owns this tree once WindowMenu registers it.
        let (_, popup) = Linear::create(LinearParameters::vertical(rows));
        (self.label, popup)
    }
}

/// Concrete collection of top-level menus installed into one window menu.
pub struct MenuPanel {
    /// Menus transferred into this panel in heading order.
    menus: Vec<Menu>,
}

impl MenuPanel {
    /// Creates one concrete panel from top-level menus.
    pub fn new(menus: impl IntoIterator<Item = Menu>) -> Self {
        // The panel is only an ownership/composition boundary; it carries no generic semantic data.
        Self { menus: menus.into_iter().collect() }
    }

    /// Returns the number of top-level menus waiting to be installed.
    pub fn len(&self) -> usize {
        self.menus.len()
    }

    /// Returns whether no top-level menu is configured.
    pub fn is_empty(&self) -> bool {
        self.menus.is_empty()
    }

    /// Transfers all concrete menus into WindowMenu construction.
    fn into_menus(self) -> Vec<Menu> {
        self.menus
    }
}

/// Generic event-registration adapter kept above the concrete menu model.
struct MenuItemBinding<State> {
    /// Finds the coordinating menu in application state when this item submits.
    menu: WindowMenuAccessor<State>,
    /// Application method registered specifically for this item's event source.
    handler: for<'a> fn(&mut State, &mut EventContext<'a>, &MenuItemSubmitted),
}

/// Generic popup-registration adapter kept above the concrete menu model.
struct PopupBinding<State> {
    /// Finds the coordinating menu in application state after popup dismissal.
    menu: WindowMenuAccessor<State>,
    /// Concrete popup position whose lifecycle source owns this binding.
    index: usize,
}

/// Application-owned coordinator for one concrete menu bar and its concrete popup roots.
///
/// Each top-level Menu becomes one retained popup tree. WindowMenu stores only root and bar handles;
/// item state and item identity remain in the concrete widgets created by the application.
/// [`Context`] owns the window and popup trees. Directly changing their visibility bypasses this
/// coordinator and can desynchronize [`Self::active_menu`] from the visible root and bar highlight.
/// Destroying any of those roots invalidates the component; discard the `WindowMenu` instead of
/// calling it afterward.
pub struct WindowMenu {
    /// Ordinary application window containing the persistent bar and caller content.
    window: RootHandle,
    /// One hidden retained popup root for each top-level menu.
    popups: Vec<PopupHandle>,
    /// Weak typed access used to reconcile the highlighted heading.
    bar: TypedWidgetHandle<MenuBar>,
    /// Currently visible top-level menu index.
    active_menu: Option<usize>,
    /// Test-only identity of the bar node inside the ordinary window.
    #[cfg(test)]
    bar_node: crate::ui_node::RuntimeNodeId,
}

impl WindowMenu {
    /// Registers one concrete menu item and its application handler.
    ///
    /// Registration should happen after MenuItem::create and before its node is moved into a
    /// MenuGroup. The adapter closes the active popup first, then invokes the supplied handler.
    /// State and renderer generics exist only at this context API boundary; every menu type remains
    /// concrete.
    ///
    /// Enabled items that are not registered have no coordinated action: their unconsumed event is
    /// discarded and selecting them does not close the active popup.
    pub fn register_item<B, State>(
        context: &mut Context<B, State>,
        menu: WindowMenuAccessor<State>,
        item: &TypedWidgetHandle<MenuItem>,
        handler: for<'a> fn(&mut State, &mut EventContext<'a>, &MenuItemSubmitted),
    ) -> Result<(), SubscribeError>
    where
        B: RendererBackend,
        State: 'static,
    {
        // Bind only function pointers and the item's concrete event port. No command object or
        // type-erased payload enters the retained menu tree.
        context.subscribe_context_with(item.submitted(), MenuItemBinding { menu, handler }, Self::dispatch_item::<State>)
    }

    /// Creates a retained window from an already-built concrete menu panel and body node.
    ///
    /// The returned component must be stored at the location identified by accessor before the
    /// first Context update, matching the other application-owned retained components.
    pub fn create<B, State>(
        context: &mut Context<B, State>,
        accessor: WindowMenuAccessor<State>,
        name: &str,
        rect: Recti,
        panel: MenuPanel,
        content: Node,
    ) -> Self
    where
        B: RendererBackend,
        State: 'static,
    {
        // Consume the concrete hierarchy once. Every top-level menu becomes its own retained popup,
        // so opening a menu only changes root visibility and never rebuilds or copies item data.
        let menus: Vec<(String, Node)> = panel.into_menus().into_iter().map(Menu::into_popup).collect();
        let labels = menus.iter().map(|(label, _)| label.clone()).collect();
        let (bar, bar_node, bar_submitted) = MenuBar::create(labels);
        #[cfg(test)]
        let bar_node_id = bar_node.id();

        // Mount the bar above the caller's body using the existing retained linear container.
        let (_, shell) = Linear::create(LinearParameters::vertical([LinearItem::content(bar_node), LinearItem::flex(content, 1.0)]));
        let window = context.create_window(name, rect, shell);

        // Register one independently retained popup per concrete top-level menu.
        let popups: Vec<PopupHandle> = menus
            .into_iter()
            .map(|(label, popup)| context.create_popup(&format!("{name} {label} Menu"), popup))
            .collect();

        let component = Self {
            window,
            popups: popups.clone(),
            bar,
            active_menu: None,
            #[cfg(test)]
            bar_node: bar_node_id,
        };

        // Popup lifecycle handlers are registered before the bar handler. An outside press that
        // reaches another heading therefore reconciles the old popup before that heading opens.
        for (index, popup) in popups.into_iter().enumerate() {
            context
                .subscribe_with(popup.submitted(), PopupBinding { menu: accessor, index }, Self::dispatch_popup::<State>)
                .expect("new window-menu popup source must be unsubscribed");
        }
        context
            .subscribe_context_with(bar_submitted, accessor, Self::dispatch_bar::<State>)
            .expect("new window-menu bar source must be unsubscribed");

        component
    }

    /// Returns a weak handle to the Context-owned window containing the menu bar and caller content.
    pub fn window(&self) -> &RootHandle {
        &self.window
    }

    /// Returns weak typed handles to all Context-owned top-level popup roots in heading order.
    pub fn popups(&self) -> &[PopupHandle] {
        // Expose only typed popup capabilities so callers cannot lose the root-kind proof required
        // by Context::show_popup_at.
        &self.popups
    }

    /// Returns a weak typed handle to one top-level popup root by heading index.
    pub fn popup(&self, index: usize) -> Option<&PopupHandle> {
        // Preserve the typed capability when selecting one menu root by its stable heading order.
        self.popups.get(index)
    }

    /// Returns the visible top-level menu index, if any.
    pub const fn active_menu(&self) -> Option<usize> {
        self.active_menu
    }

    /// Returns whether one top-level menu is currently visible.
    pub const fn is_open(&self) -> bool {
        self.active_menu.is_some()
    }

    /// Closes the active menu from ordinary application code.
    ///
    /// # Panics
    ///
    /// Panics if the component's popup roots or retained menu bar were destroyed independently.
    pub fn close<B, State>(&mut self, context: &mut Context<B, State>)
    where
        B: RendererBackend,
        State: 'static,
    {
        // Root visibility and bar presentation are reconciled as one synchronous operation.
        if let Some(index) = self.active_menu {
            context
                .set_root_visible(self.popups[index].id(), false)
                .expect("window-menu popup must remain registered");
        }
        self.finish_close();
    }

    /// Applies one bar request at the borrow-safe context event boundary.
    fn bar_submitted(&mut self, context: &mut EventContext<'_>, event: &MenuBarSubmitted) {
        if !event.open {
            self.close_from_event(context);
            return;
        }

        let Some(popup) = self.popups.get(event.index) else {
            // A stale or invalid bar index cannot name a popup, so normalize the component closed.
            self.finish_close();
            return;
        };
        // The persistent bar belongs to this component's ordinary window, so that window is the
        // authoritative layer initiator for every top-level menu popup. A low-layer application
        // surface therefore keeps its menus below unrelated roots in higher application layers.
        context
            .show_popup_at(popup, self.window.id(), event.anchor)
            .expect("window-menu popup and its initiating window must remain registered");
        self.active_menu = Some(event.index);
        self.bar
            .try_update(|bar| bar.set_open_menu(Some(event.index)))
            .expect("window-menu bar must remain mounted");
    }

    /// Closes the active popup through an event-time root mutation capability.
    fn close_from_event(&mut self, context: &mut EventContext<'_>) {
        if let Some(index) = self.active_menu {
            context
                .set_root_visible(self.popups[index].id(), false)
                .expect("window-menu popup must remain registered");
        }
        self.finish_close();
    }

    /// Reconciles state after one concrete popup is dismissed by window-manager policy.
    fn popup_submitted(&mut self, index: usize, event: &RootSubmitted) {
        if matches!(event, RootSubmitted::PopupDismissed) && self.active_menu == Some(index) {
            // The root is already hidden; only application-owned and bar state require mutation.
            self.finish_close();
        }
    }

    /// Clears every non-root representation of an open menu.
    fn finish_close(&mut self) {
        self.active_menu = None;
        self.bar.try_update(|bar| bar.set_open_menu(None)).expect("window-menu bar must remain mounted");
    }

    /// Adapts one registered concrete item's event to popup closure and its application method.
    fn dispatch_item<State>(state: &mut State, binding: &MenuItemBinding<State>, context: &mut EventContext<'_>, event: &MenuItemSubmitted) {
        // End the component borrow before invoking arbitrary application code with the same state.
        (binding.menu)(state).close_from_event(context);
        (binding.handler)(state, context, event);
    }

    /// Adapts the private concrete bar event to the application-owned component.
    fn dispatch_bar<State>(state: &mut State, accessor: &WindowMenuAccessor<State>, context: &mut EventContext<'_>, event: &MenuBarSubmitted) {
        accessor(state).bar_submitted(context, event);
    }

    /// Adapts one concrete popup lifecycle event to the application-owned component.
    fn dispatch_popup<State>(state: &mut State, binding: &PopupBinding<State>, event: &RootSubmitted) {
        (binding.menu)(state).popup_submitted(binding.index, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{NoopRenderer, test_atlas};
    use crate::{Dimensioni, MouseButton, RootChrome, TextBlock, TextBlockParameters, rect};

    /// Application state for complete concrete menu behavior tests.
    struct Model {
        /// Concrete menu coordinator found by every registered item and internal event.
        menu: WindowMenu,
        /// Item-specific handlers append their own semantic result directly.
        invoked: Vec<&'static str>,
        /// Live state handle retained only for the disabled Save item.
        save: TypedWidgetHandle<MenuItem>,
    }

    impl Model {
        /// Resolves the stable menu component location for context registrations.
        fn menu_mut(state: &mut Self) -> &mut WindowMenu {
            &mut state.menu
        }

        /// Handles the New item's concrete event source.
        fn new_submitted(&mut self, _context: &mut EventContext<'_>, _event: &MenuItemSubmitted) {
            self.invoked.push("New");
        }

        /// Handles the Save item's concrete event source.
        fn save_submitted(&mut self, _context: &mut EventContext<'_>, _event: &MenuItemSubmitted) {
            self.invoked.push("Save");
        }
    }

    /// Constructs registered concrete items, composes them, and installs the resulting window.
    fn context_and_model() -> (Context<NoopRenderer, Model>, Model) {
        let mut context = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(480, 320));

        let (new_item, new_node) = MenuItem::create(MenuItemParameters::new("New").shortcut_hint("Ctrl+N"));
        WindowMenu::register_item(&mut context, Model::menu_mut, &new_item, Model::new_submitted).unwrap();

        let (save, save_node) = MenuItem::create(MenuItemParameters::new("Save").disabled());
        WindowMenu::register_item(&mut context, Model::menu_mut, &save, Model::save_submitted).unwrap();

        let (word_wrap, word_wrap_node) = MenuItem::create(MenuItemParameters::new("Word Wrap").checked(true));
        let (about, about_node) = MenuItem::create(MenuItemParameters::new("About"));

        let panel = MenuPanel::new([
            Menu::new("File", [MenuGroup::new([new_node, save_node])]),
            Menu::new("View", [MenuGroup::new([word_wrap_node]), MenuGroup::new([about_node])]),
        ]);
        let body = TextBlock::create(TextBlockParameters::new("body")).1;
        let menu = WindowMenu::create(&mut context, Model::menu_mut, "Menu Window", rect(20, 20, 260, 180), panel, body);
        let model = Model { menu, invoked: Vec::new(), save };
        // Keep the unused test handle alive only to prove concrete item identity does not require
        // application storage when no live state mutation is needed.
        drop((new_item, word_wrap, about));
        (context, model)
    }

    /// Clicks one screen-space point and commits press and release separately.
    fn click(context: &mut Context<NoopRenderer, Model>, model: &mut Model, x: i32, y: i32) {
        context.mousemove(x, y);
        context.mousedown(x, y, MouseButton::LEFT);
        context.update_ui_state(Dimensioni::new(480, 320), model);
        context.mouseup(x, y, MouseButton::LEFT);
        context.update_ui_state(Dimensioni::new(480, 320), model);
    }

    #[test]
    fn concrete_builders_preserve_only_owned_structure() {
        let first = TextBlock::create(TextBlockParameters::new("first")).1;
        let second = TextBlock::create(TextBlockParameters::new("second")).1;
        let group = MenuGroup::new([first, second]);
        assert_eq!(group.len(), 2);
        let menu = Menu::new("File", [group]);
        assert_eq!(menu.label(), "File");
        assert_eq!(menu.len(), 1);
        let panel = MenuPanel::new([menu]);
        assert_eq!(panel.len(), 1);
    }

    #[test]
    fn concrete_item_handle_updates_live_state() {
        let (context, model) = context_and_model();
        assert_eq!(model.save.is_enabled(), Some(false));
        assert_eq!(model.save.set_enabled(true), Some(()));
        assert_eq!(model.save.is_enabled(), Some(true));
        // Context remains the sole strong owner after composition.
        assert!(model.save.is_alive());
        drop(context);
        assert!(!model.save.is_alive());
    }

    #[test]
    fn registered_item_invokes_its_handler_and_closes_popup() {
        let (mut context, mut model) = context_and_model();
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        let bar = context.debug_root_node_rect(model.menu.window.id(), model.menu.bar_node).unwrap();
        click(&mut context, &mut model, bar.x + 8, bar.y + bar.height / 2);

        let popup = model.menu.popup(0).unwrap().clone();
        let popup_rect = popup.widget().try_read(|root| root.rect()).unwrap();
        click(
            &mut context,
            &mut model,
            popup_rect.x + popup_rect.width / 2,
            popup_rect.y + popup_rect.height / 4,
        );

        assert_eq!(model.invoked, ["New"]);
        assert!(!model.menu.is_open());
        assert_eq!(popup.widget().try_read(RootChrome::is_visible), Some(false));
    }

    #[test]
    fn popup_dismissal_reconciles_component_and_bar_state() {
        let (mut context, mut model) = context_and_model();
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        let bar = context.debug_root_node_rect(model.menu.window.id(), model.menu.bar_node).unwrap();
        click(&mut context, &mut model, bar.x + 8, bar.y + bar.height / 2);
        assert!(model.menu.is_open());

        click(&mut context, &mut model, 460, 300);
        assert!(!model.menu.is_open());
        assert_eq!(model.menu.bar.try_read(MenuBar::open_menu), Some(None));
        assert_eq!(model.menu.popup(0).unwrap().widget().try_read(RootChrome::is_visible), Some(false),);
    }
}
