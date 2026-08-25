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
use crate::ui_node::widgets::{MenuBar, MenuBarSubmitted, MenuList, MenuSeparator, MenuSubmenu, MenuSubmenuSubmitted};
use crate::{
    Context, EventContext, Linear, LinearItem, LinearParameters, Node, PopupHandle, Recti, RootHandle, RootSubmitted, SubscribeError, TypedWidgetHandle,
    WidgetEventPortHandle, WindowOption,
};
pub use crate::ui_node::widgets::{MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted};

/// Accessor retained by context subscriptions to locate one application-owned menu.
type WindowMenuAccessor<State> = for<'a> fn(&'a mut State) -> &'a mut WindowMenu;

/// One entry in a concrete menu group.
enum MenuEntry {
    /// An application-created actionable item node.
    Item(Node),
    /// A recursively composed cascading submenu.
    Submenu(Submenu),
}

/// One recursively composable cascading submenu.
pub struct Submenu {
    /// User-visible label displayed in its parent menu row.
    label: String,
    /// Concrete groups transferred into the submenu popup in display order.
    groups: Vec<MenuGroup>,
}

impl Submenu {
    /// Creates a submenu from a parent-row label and its retained groups.
    pub fn new(label: impl Into<String>, groups: impl IntoIterator<Item = MenuGroup>) -> Self {
        Self {
            label: label.into(),
            groups: groups.into_iter().collect(),
        }
    }

    /// Returns the user-visible parent-row label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the number of groups waiting to be mounted.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Returns whether this submenu contains no groups.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// One concrete separator-delimited group of retained menu items and submenus.
pub struct MenuGroup {
    /// Unique unmounted entries transferred into this group in display order.
    entries: Vec<MenuEntry>,
}

impl MenuGroup {
    /// Creates a group from concrete retained menu item nodes.
    pub fn new(items: impl IntoIterator<Item = Node>) -> Self {
        // Collect once at the ownership boundary; no description is cloned when a menu opens.
        Self {
            entries: items.into_iter().map(MenuEntry::Item).collect(),
        }
    }

    /// Creates a group containing one cascading submenu.
    pub fn submenu(submenu: Submenu) -> Self {
        Self {
            entries: vec![MenuEntry::Submenu(submenu)],
        }
    }

    /// Appends a cascading submenu after this group's existing entries.
    pub fn with_submenu(mut self, submenu: Submenu) -> Self {
        self.entries.push(MenuEntry::Submenu(submenu));
        self
    }

    /// Returns the number of retained items and submenus waiting to be mounted.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether this group contains no items or submenus.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Transfers this group's unique entries to its concrete menu.
    fn into_entries(self) -> Vec<MenuEntry> {
        self.entries
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
}

/// Concrete collection of top-level menus installed into one window menu.
pub struct MenuPanel {
    /// Menus transferred into this panel in heading order.
    menus: Vec<Menu>,
}

/// One compiled popup tree waiting to be installed under its owning window.
struct PopupDefinition {
    /// Index of the parent popup, or `None` when the application window owns this level directly.
    parent: Option<usize>,
    /// Label path used only for the popup's diagnostic name.
    path: String,
    /// Uniquely owned retained menu surface.
    content: Node,
}

/// One compiled submenu row binding its parent popup to its child popup.
struct SubmenuDefinition {
    /// Index of the popup opened by this submenu row.
    child: usize,
    /// Concrete row event used to install the retained opening handler.
    submitted: WidgetEventPortHandle<MenuSubmenuSubmitted>,
}

/// Flattened retained popup definitions produced from a recursively composed menu panel.
struct CompiledMenus {
    /// Top-level labels mounted into the persistent menu bar.
    labels: Vec<String>,
    /// Parent-before-child popup definitions; slots are reserved before recursive compilation.
    popups: Vec<Option<PopupDefinition>>,
    /// Popup indices corresponding one-to-one with `labels`.
    top_level: Vec<usize>,
    /// Retained submenu-row event sources and the child popup each row opens.
    submenus: Vec<SubmenuDefinition>,
}

impl CompiledMenus {
    /// Consumes the public recursive menu description into directly installable retained popups.
    fn new(panel: MenuPanel) -> Self {
        let mut compiled = Self {
            labels: Vec::new(),
            popups: Vec::new(),
            top_level: Vec::new(),
            submenus: Vec::new(),
        };
        for menu in panel.into_menus() {
            compiled.labels.push(menu.label.clone());
            let path = menu.label.clone();
            // A top-level menu popup is owned directly by the application window created later.
            let popup = compiled.push_popup(None, path, menu.groups);
            compiled.top_level.push(popup);
        }
        compiled
    }

    /// Compiles one popup and recursively appends its descendants in parent-before-child order.
    fn push_popup(&mut self, parent: Option<usize>, path: String, groups: Vec<MenuGroup>) -> usize {
        // Reserve the current slot before descending. This gives every child a stable parent index
        // while preserving the parent-before-child order required by popup installation.
        let index = self.popups.len();
        self.popups.push(None);

        let mut rows = Vec::new();
        for group in groups {
            let entries = group.into_entries();
            if entries.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(MenuSeparator::create());
            }
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) => rows.push(item),
                    MenuEntry::Submenu(submenu) => {
                        let child_path = format!("{path} {}", submenu.label);
                        let child = self.push_popup(Some(index), child_path, submenu.groups);
                        let (row, submitted) = MenuSubmenu::create(submenu.label);
                        rows.push(row);
                        self.submenus.push(SubmenuDefinition { child, submitted });
                    }
                }
            }
        }
        // Fill the reserved slot after recursion. The vector order, rather than the fill order,
        // remains parent-before-child and therefore needs no secondary tree representation.
        self.popups[index] = Some(PopupDefinition {
            parent,
            path,
            content: MenuList::create(rows),
        });
        index
    }
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
    /// Top-level heading represented by this popup, absent for submenu popups.
    top_level: Option<usize>,
}

/// Generic submenu-row adapter kept above the concrete recursive model.
struct SubmenuBinding<State> {
    /// Finds the coordinating menu when this retained row submits.
    menu: WindowMenuAccessor<State>,
    /// Index of the popup whose stable owner is already recorded by Context.
    child: usize,
}

/// Application-owned coordinator for one concrete menu bar and its window-owned popups.
///
/// Each Menu and Submenu becomes one retained popup owned by the application window. WindowMenu
/// stores only weak window, popup, and bar handles; item state and item identity remain in the
/// concrete widgets created by the application. Top-level menus belong directly to the window,
/// while each submenu belongs to its parent popup. Showing or hiding these popups outside this
/// coordinator can desynchronize [`Self::active_menu`] from the visible popup and bar highlight.
/// Destroying the window releases every owned popup, after which this component must not be used.
pub struct WindowMenu {
    /// Ordinary application window containing the persistent bar and caller content.
    window: RootHandle,
    /// One hidden retained popup for each top-level menu.
    popups: Vec<PopupHandle>,
    /// Every top-level and cascading popup in flattened construction order.
    all_popups: Vec<PopupHandle>,
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
        // Consume the concrete hierarchy once. Every menu level becomes its own retained popup, so
        // opening either a heading or submenu only changes popup visibility.
        let CompiledMenus {
            labels,
            popups: definitions,
            top_level,
            submenus,
        } = CompiledMenus::new(panel);
        let (bar, bar_node, bar_submitted) = MenuBar::create(labels);
        #[cfg(test)]
        let bar_node_id = bar_node.id();

        // Mount the bar above the caller's body using the existing retained linear container.
        let (_, shell) = Linear::create(LinearParameters::vertical([LinearItem::content(bar_node), LinearItem::flex(content, 1.0)]));
        let window = context.create_window(name, rect, shell);

        // Install every popup up front. Menu surfaces own their full background, so remove the
        // generic popup content inset and let only popup framing surround that color.
        let mut all_popups: Vec<PopupHandle> = Vec::with_capacity(definitions.len());
        for definition in definitions {
            let definition = definition.expect("recursive menu compilation must fill every reserved popup");
            // Definitions are parent-before-child. Install a top-level popup under the window and a
            // submenu under the already-installed popup named by its stable parent index.
            let popup_name = format!("{name} {} Menu", definition.path);
            let popup = if let Some(parent) = definition.parent {
                context.create_subpopup(&all_popups[parent], &popup_name, definition.content)
            } else {
                context.create_popup(window.id(), &popup_name, definition.content)
            }
            .expect("new window-menu popup owner must remain registered");
            context
                .set_popup_options(
                    &popup,
                    WindowOption::FRAME | WindowOption::AUTO_SIZE | WindowOption::NO_RESIZE | WindowOption::NO_TITLE | WindowOption::NO_PADDING,
                )
                .expect("new window-menu popup must remain alive");
            all_popups.push(popup);
        }
        let popups = top_level.iter().map(|index| all_popups[*index].clone()).collect();

        let component = Self {
            window,
            popups,
            all_popups: all_popups.clone(),
            bar,
            active_menu: None,
            #[cfg(test)]
            bar_node: bar_node_id,
        };

        // Popup lifecycle handlers are registered before the bar handler. An outside press that
        // reaches another heading therefore reconciles the old popup before that heading opens.
        for (index, popup) in all_popups.iter().enumerate() {
            let top_level = top_level.iter().position(|popup_index| *popup_index == index);
            context
                .subscribe_with(popup.submitted(), PopupBinding { menu: accessor, top_level }, Self::dispatch_popup::<State>)
                .expect("new window-menu popup source must be unsubscribed");
        }
        for submenu in submenus {
            context
                .subscribe_context_with(
                    submenu.submitted,
                    SubmenuBinding { menu: accessor, child: submenu.child },
                    Self::dispatch_submenu::<State>,
                )
                .expect("new window-menu submenu source must be unsubscribed");
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

    /// Returns weak handles to all window-owned top-level popups in heading order.
    pub fn popups(&self) -> &[PopupHandle] {
        // Expose only popup capabilities so callers cannot apply window-only mutations to these
        // transient surfaces.
        &self.popups
    }

    /// Returns a weak handle to one top-level popup by heading index.
    pub fn popup(&self, index: usize) -> Option<&PopupHandle> {
        // Preserve the popup capability when selecting one menu by its stable heading order.
        self.popups.get(index)
    }

    /// Returns every window-owned menu and submenu popup in parent-before-descendant order.
    ///
    /// Use this complete view for inspection. Whole-component teardown needs only the owning
    /// window, which releases all of its popups. Use [`Self::popups`] when indices need to correspond
    /// to top-level headings.
    pub fn all_popups(&self) -> &[PopupHandle] {
        &self.all_popups
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
    /// Panics if the component's popups or retained menu bar are no longer alive.
    pub fn close<B, State>(&mut self, context: &mut Context<B, State>)
    where
        B: RendererBackend,
        State: 'static,
    {
        // Popup visibility and bar presentation are reconciled as one synchronous operation.
        if let Some(index) = self.active_menu {
            context.hide_popup(&self.popups[index]).expect("window-menu popup must remain alive");
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
        // Stable ownership already binds every top-level menu popup to this window. Opening only
        // supplies the event's screen-space anchor; it does not rebuild menu ancestry.
        context
            .show_popup_at(popup, event.anchor)
            .expect("window-menu popup and its owning window must remain alive");
        self.active_menu = Some(event.index);
        self.bar
            .try_update(|bar| bar.set_open_menu(Some(event.index)))
            .expect("window-menu bar must remain mounted");
    }

    /// Closes the active popup through an event-time popup mutation capability.
    fn close_from_event(&mut self, context: &mut EventContext<'_>) {
        if let Some(index) = self.active_menu {
            context.hide_popup(&self.popups[index]).expect("window-menu popup must remain alive");
        }
        self.finish_close();
    }

    /// Opens one child popup beside its parent while retaining the complete ancestor chain.
    fn submenu_submitted(&mut self, context: &mut EventContext<'_>, child: usize, event: &MenuSubmenuSubmitted) {
        if self.active_menu.is_none() {
            return;
        }
        let Some(child) = self.all_popups.get(child) else {
            return;
        };
        // Context follows the child's stable parent link to retain the ancestor branch. The menu
        // coordinator only contributes the placement captured by the concrete row event.
        context
            .show_popup_at(child, event.anchor)
            .expect("window-menu submenu and its owning popup must remain alive");
    }

    /// Reconciles state after one concrete popup is dismissed by window-manager policy.
    fn popup_submitted(&mut self, top_level: Option<usize>, event: &RootSubmitted) {
        if matches!(event, RootSubmitted::PopupDismissed) && top_level.is_some_and(|index| self.active_menu == Some(index)) {
            // The popup is already hidden; only application-owned and bar state require mutation.
            self.finish_close();
        }
    }

    /// Clears every non-popup representation of an open menu.
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
        (binding.menu)(state).popup_submitted(binding.top_level, event);
    }

    /// Adapts one private submenu-row event to the application-owned component.
    fn dispatch_submenu<State>(state: &mut State, binding: &SubmenuBinding<State>, context: &mut EventContext<'_>, event: &MenuSubmenuSubmitted) {
        (binding.menu)(state).submenu_submitted(context, binding.child, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{NoopRenderer, test_atlas};
    use crate::{Dimensioni, MouseButton, TextBlock, TextBlockParameters, rect};

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

        /// Handles an item nested inside a cascading submenu.
        fn about_submitted(&mut self, _context: &mut EventContext<'_>, _event: &MenuItemSubmitted) {
            self.invoked.push("About");
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
        WindowMenu::register_item(&mut context, Model::menu_mut, &about, Model::about_submitted).unwrap();

        let panel = MenuPanel::new([
            Menu::new("File", [MenuGroup::new([new_node, save_node])]),
            Menu::new(
                "View",
                [
                    MenuGroup::new([word_wrap_node]),
                    MenuGroup::submenu(Submenu::new("Details", [MenuGroup::new([about_node])])),
                ],
            ),
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
        let popup_rect = context.debug_popup_rect(&popup).unwrap();
        click(
            &mut context,
            &mut model,
            popup_rect.x + popup_rect.width / 2,
            popup_rect.y + popup_rect.height / 4,
        );

        assert_eq!(model.invoked, ["New"]);
        assert!(!model.menu.is_open());
        assert_eq!(context.debug_popup_visible(&popup), Some(false));
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
        assert_eq!(context.debug_popup_visible(model.menu.popup(0).unwrap()), Some(false));
    }

    #[test]
    fn submenu_retains_its_parent_and_nested_item_closes_the_complete_chain() {
        let (mut context, mut model) = context_and_model();
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        let bar = context.debug_root_node_rect(model.menu.window.id(), model.menu.bar_node).unwrap();

        // File occupies the first compact heading; the next point opens View.
        click(&mut context, &mut model, bar.x + 55, bar.y + bar.height / 2);
        let parent = model.menu.popup(1).unwrap().clone();
        let parent_rect = context.debug_popup_rect(&parent).unwrap();
        click(
            &mut context,
            &mut model,
            parent_rect.x + parent_rect.width / 2,
            parent_rect.y + parent_rect.height - 10,
        );

        let child = model.menu.all_popups[2].clone();
        assert_eq!(context.debug_popup_visible(&parent), Some(true));
        assert_eq!(context.debug_popup_visible(&child), Some(true));

        let child_rect = context.debug_popup_rect(&child).unwrap();
        click(
            &mut context,
            &mut model,
            child_rect.x + child_rect.width / 2,
            child_rect.y + child_rect.height / 2,
        );

        assert_eq!(model.invoked, ["About"]);
        assert!(!model.menu.is_open());
        assert_eq!(context.debug_popup_visible(&parent), Some(false));
        assert_eq!(context.debug_popup_visible(&child), Some(false));
    }
}
