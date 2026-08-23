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

//! Application-owned per-window menus with retained bars and popup panels.
//!
//! [`WindowMenu`] follows the same component boundary as [`crate::FileDialog`]: application state
//! owns the semantic component while [`crate::Context`] owns every retained root and widget tree.
//! The persistent [`crate::ui_node::widgets::MenuBar`] presentation is mounted above the supplied
//! window content. One independently retained popup root displays whichever top-level menu is
//! active, allowing the panel to escape window clipping and participate in ordinary window-manager
//! dismissal, z-order, modal, layout, and paint policy.
//!
//! This initial menu contract supports multiple top-level menus, separator-delimited groups,
//! enabled and disabled commands, check and radio marks, shortcut presentation hints, live state
//! mutation, and typed application command delivery. Shortcut activation and cascading submenus
//! require the planned logical-key and popup-family work and are intentionally not simulated by
//! text input or by frame-polled application flags.

use std::{cell::RefCell, rc::Rc};

use crate::{Context, EventContext, Linear, LinearItem, LinearParameters, Node, Recti, RootHandle, RootSubmitted, TypedWidgetHandle, WidgetEventPortHandle};
use crate::event::WidgetEventPort;
use crate::render::RendererBackend;
use crate::ui_node::widgets::{MenuBar, MenuBarSubmitted, MenuPanel, MenuPanelSubmitted};

/// Accessor used by internal subscriptions to find one application-owned menu component.
type WindowMenuAccessor<State, Command> = for<'a> fn(&'a mut State) -> &'a mut WindowMenu<Command>;

/// Visual state displayed in the marker column of one menu item.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MenuItemMark {
    /// The item has no persistent marker.
    None,
    /// A checkable item whose boolean indicates whether the check glyph is visible.
    Checked(bool),
    /// A radio item whose boolean indicates whether the selection marker is visible.
    Radio(bool),
}

/// One executable row in a top-level menu.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuItemSpec<Command> {
    /// User-visible label drawn in the panel's primary text column.
    label: String,
    /// Application-defined semantic value emitted when this enabled item is submitted.
    command: Command,
    /// Whether pointer submission may invoke this command.
    enabled: bool,
    /// Optional checked or radio presentation state.
    mark: MenuItemMark,
    /// Presentation-only shortcut label; shortcut dispatch is not part of the initial contract.
    shortcut_hint: Option<String>,
}

impl<Command> MenuItemSpec<Command> {
    /// Creates one enabled, unmarked item without a shortcut hint.
    pub fn new(label: impl Into<String>, command: Command) -> Self {
        Self {
            label: label.into(),
            command,
            enabled: true,
            mark: MenuItemMark::None,
            shortcut_hint: None,
        }
    }

    /// Returns the displayed item label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the application-defined semantic command.
    pub fn command(&self) -> &Command {
        &self.command
    }

    /// Returns whether this item accepts submission.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the current check/radio presentation state.
    pub const fn marker(&self) -> MenuItemMark {
        self.mark
    }

    /// Returns the presentation-only shortcut label, when configured.
    pub fn shortcut_hint(&self) -> Option<&str> {
        self.shortcut_hint.as_deref()
    }

    /// Replaces initial enabled state.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Marks this item disabled during construction.
    pub const fn disabled(self) -> Self {
        self.enabled(false)
    }

    /// Replaces initial marker state.
    pub const fn with_mark(mut self, mark: MenuItemMark) -> Self {
        self.mark = mark;
        self
    }

    /// Configures this item as checked or unchecked.
    pub const fn checked(self, checked: bool) -> Self {
        self.with_mark(MenuItemMark::Checked(checked))
    }

    /// Configures this item as a selected or unselected radio choice.
    pub const fn radio(self, selected: bool) -> Self {
        self.with_mark(MenuItemMark::Radio(selected))
    }

    /// Adds a display hint such as `Ctrl+S` to the panel's right-aligned shortcut column.
    ///
    /// The current raw-input API cannot represent arbitrary logical character keys, so this value
    /// is deliberately presentation-only. A later accelerator API can reuse the visual column while
    /// replacing this string with a typed key chord.
    pub fn with_shortcut_hint(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut_hint = Some(shortcut.into());
        self
    }

    /// Mutates enabled state without changing command identity or presentation metadata.
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Mutates marker state without recording a user command submission.
    fn set_mark(&mut self, mark: MenuItemMark) {
        self.mark = mark;
    }
}

/// One row in a menu panel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MenuEntry<Command> {
    /// An executable application command row.
    Item(MenuItemSpec<Command>),
    /// A non-interactive visual boundary between adjacent command groups.
    Separator,
}

impl<Command> MenuEntry<Command> {
    /// Creates one enabled command item.
    pub fn item(label: impl Into<String>, command: Command) -> Self {
        Self::Item(MenuItemSpec::new(label, command))
    }

    /// Creates one separator group boundary.
    pub const fn separator() -> Self {
        Self::Separator
    }

    /// Returns whether this entry is a separator.
    pub const fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }

    /// Returns the item specification when this is an executable row.
    pub fn as_item(&self) -> Option<&MenuItemSpec<Command>> {
        match self {
            Self::Item(item) => Some(item),
            Self::Separator => None,
        }
    }

    /// Returns mutable item state when this is an executable row.
    fn item_mut(&mut self) -> Option<&mut MenuItemSpec<Command>> {
        match self {
            Self::Item(item) => Some(item),
            Self::Separator => None,
        }
    }

    /// Replaces initial enabled state when this is an item.
    pub fn enabled(mut self, enabled: bool) -> Self {
        if let Some(item) = self.item_mut() {
            item.enabled = enabled;
        }
        self
    }

    /// Marks this item disabled during construction; separators remain unchanged.
    pub fn disabled(self) -> Self {
        self.enabled(false)
    }

    /// Replaces initial marker state when this is an item.
    pub fn mark(mut self, mark: MenuItemMark) -> Self {
        if let Some(item) = self.item_mut() {
            item.mark = mark;
        }
        self
    }

    /// Configures this item as checked or unchecked.
    pub fn checked(self, checked: bool) -> Self {
        self.mark(MenuItemMark::Checked(checked))
    }

    /// Configures this item as a selected or unselected radio choice.
    pub fn radio(self, selected: bool) -> Self {
        self.mark(MenuItemMark::Radio(selected))
    }

    /// Adds a presentation-only shortcut hint when this is an item.
    pub fn shortcut_hint(mut self, shortcut: impl Into<String>) -> Self {
        if let Some(item) = self.item_mut() {
            item.shortcut_hint = Some(shortcut.into());
        }
        self
    }
}

/// One named top-level menu and its ordered panel entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuSpec<Command> {
    /// Label displayed in the persistent window-local menu bar.
    label: String,
    /// Ordered executable rows and separator-delimited groups displayed when open.
    entries: Vec<MenuEntry<Command>>,
}

impl<Command> MenuSpec<Command> {
    /// Creates a named top-level menu from ordered entries.
    pub fn new<T>(label: impl Into<String>, entries: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<MenuEntry<Command>>,
    {
        Self {
            label: label.into(),
            entries: entries.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the top-level bar label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the complete ordered menu contents.
    pub fn entries(&self) -> &[MenuEntry<Command>] {
        &self.entries
    }
}

/// Complete semantic definition of one window's menu bar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuBarSpec<Command> {
    /// Top-level menus rendered from left to right in construction order.
    menus: Vec<MenuSpec<Command>>,
}

impl<Command> MenuBarSpec<Command> {
    /// Creates a menu bar from ordered top-level menus.
    pub fn new<T>(menus: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<MenuSpec<Command>>,
    {
        Self {
            menus: menus.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the top-level menu definitions in bar order.
    pub fn menus(&self) -> &[MenuSpec<Command>] {
        &self.menus
    }
}

/// Application-facing event emitted after an enabled menu item is selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MenuInvoked<Command: 'static> {
    /// Application-defined command cloned from the selected item specification.
    pub command: Command,
}

impl<Command: 'static> crate::WidgetEvent for MenuInvoked<Command> {}

/// Application-owned coordinator for one retained window menu and its popup panel.
///
/// Construct the component once, store it at the location returned by the supplied application
/// accessor, subscribe to [`Self::invoked`], and drive the context through
/// [`Context::update_ui_state`]. The accessor is retained only by typed subscriptions and is not
/// invoked during construction, matching the established [`crate::FileDialog`] component pattern.
///
/// The menu creates and owns handles for two Context-owned roots: [`Self::window`] is the ordinary
/// application window containing the bar and supplied content, while [`Self::popup`] is the hidden
/// auto-sized panel root. Destroying either root independently expires its retained handles;
/// applications should normally keep both alive for the component lifetime.
pub struct WindowMenu<Command: Clone + 'static> {
    /// Authoritative mutable menu definition retained across popup activations.
    specification: MenuBarSpec<Command>,
    /// Ordinary application window that owns the persistent menu bar.
    window: RootHandle,
    /// Hidden auto-sized root reused for every top-level menu panel.
    popup: RootHandle,
    /// Weak typed access to bar semantic and presentation state.
    bar: TypedWidgetHandle<MenuBar>,
    /// Weak typed access to the popup panel's current entry snapshot.
    panel: TypedWidgetHandle<MenuPanel<Command>>,
    /// Currently visible top-level menu index, or `None` while the panel is closed.
    active_menu: Option<usize>,
    /// Width of the heading anchor used to synchronize mutations into an already-open panel.
    active_minimum_width: i32,
    /// Stable component-owned application command source.
    invoked_event: Rc<RefCell<WidgetEventPort<MenuInvoked<Command>>>>,
    /// Test-only identity of the bar node within the ordinary window tree.
    #[cfg(test)]
    bar_node: crate::ui_node::RuntimeNodeId,
    /// Test-only identity of the panel node within the popup tree.
    #[cfg(test)]
    panel_node: crate::ui_node::RuntimeNodeId,
}

impl<Command: Clone + 'static> WindowMenu<Command> {
    /// Creates one ordinary retained window with a menu bar above `content`.
    ///
    /// Internal bar, panel, and popup-lifecycle events are connected to `accessor` immediately.
    /// The returned component itself must then be installed at that accessor's location before the
    /// first [`Context::update_ui_state`] call. As with other retained topology construction, both
    /// the bar and `content` transfer their unique [`Node`] ownership into the new window tree.
    pub fn create<B, State>(
        context: &mut Context<B, State>,
        accessor: for<'a> fn(&'a mut State) -> &'a mut Self,
        name: &str,
        rect: Recti,
        specification: MenuBarSpec<Command>,
        content: Node,
    ) -> Self
    where
        B: RendererBackend,
        State: 'static,
    {
        // The bar stores only top-level labels. Complete command definitions stay in this component
        // and are copied into the independent popup tree only while one menu is visible.
        let labels = specification.menus.iter().map(|menu| menu.label.clone()).collect();
        let (bar, bar_node, bar_submitted) = MenuBar::create(labels);
        #[cfg(test)]
        let bar_node_id = bar_node.id();

        // RootChrome intentionally owns one application node. A vertical retained shell preserves
        // that invariant while assigning content-sized height to the bar and all remaining height
        // to the caller's application body.
        let (_, shell) = Linear::create(LinearParameters::vertical([LinearItem::content(bar_node), LinearItem::flex(content, 1.0)]));
        let window = context.create_window(name, rect, shell);

        // The panel is an independent popup root so it paints above other roots and is not clipped
        // to the owning window's body. One retained panel is reused as headings change.
        let (panel, panel_node, panel_submitted) = MenuPanel::create();
        #[cfg(test)]
        let panel_node_id = panel_node.id();
        let popup = context.create_popup(&format!("{name} Menu"), panel_node);

        let invoked_event = Rc::new(RefCell::new(WidgetEventPort::new()));
        let component = Self {
            specification,
            window,
            popup: popup.clone(),
            bar,
            panel,
            active_menu: None,
            active_minimum_width: 0,
            invoked_event,
            #[cfg(test)]
            bar_node: bar_node_id,
            #[cfg(test)]
            panel_node: panel_node_id,
        };

        // Dismissal is registered before bar submission intentionally. When the user clicks another
        // heading, WindowManager first dismisses the old popup and then routes that same press to
        // the bar. Subscription-order dispatch therefore closes stale state before the later bar
        // request opens the newly selected menu.
        context
            .subscribe_with(popup.submitted(), accessor, Self::dispatch_popup_submitted::<State>)
            .expect("new window-menu popup lifecycle source must be unsubscribed");
        context
            .subscribe_context_with(bar_submitted, accessor, Self::dispatch_bar_submitted::<State>)
            .expect("new window-menu bar source must be unsubscribed");
        context
            .subscribe_context_with(panel_submitted, accessor, Self::dispatch_panel_submitted::<State>)
            .expect("new window-menu panel source must be unsubscribed");

        component
    }

    /// Returns the ordinary retained window that owns this menu bar and application content.
    pub fn window(&self) -> &RootHandle {
        &self.window
    }

    /// Returns the retained popup root reused for the active menu panel.
    ///
    /// This accessor is primarily useful for inspection. Use [`Self::close`] rather than mutating
    /// popup visibility directly so component, bar, and root state stay synchronized.
    pub fn popup(&self) -> &RootHandle {
        &self.popup
    }

    /// Returns the stable typed application command endpoint owned by this component.
    pub fn invoked(&self) -> WidgetEventPortHandle<MenuInvoked<Command>> {
        WidgetEventPortHandle::new(&self.invoked_event)
    }

    /// Returns the authoritative semantic menu definition.
    pub fn specification(&self) -> &MenuBarSpec<Command> {
        &self.specification
    }

    /// Returns whether one top-level menu is currently visible.
    pub const fn is_open(&self) -> bool {
        self.active_menu.is_some()
    }

    /// Closes the active menu from ordinary application code.
    ///
    /// This operation mutates Context-owned popup visibility and retained bar state synchronously;
    /// call [`Context::update_ui_state`] before rendering the resulting UI.
    pub fn close<B, State>(&mut self, context: &mut Context<B, State>)
    where
        B: RendererBackend,
        State: 'static,
    {
        context
            .set_root_visible(self.popup.id(), false)
            .expect("application-owned window-menu popup must remain registered");
        self.finish_close();
    }

    /// Enables or disables every item whose semantic command equals `command`.
    ///
    /// The returned count allows applications to detect missing commands and intentionally supports
    /// placing the same command in more than one group. If its menu is open, the visible panel
    /// snapshot is refreshed immediately at the current safe application boundary.
    pub fn set_enabled(&mut self, command: &Command, enabled: bool) -> usize
    where
        Command: PartialEq,
    {
        self.mutate_command(command, |item| item.set_enabled(enabled))
    }

    /// Replaces the marker on every item whose semantic command equals `command`.
    pub fn set_mark(&mut self, command: &Command, mark: MenuItemMark) -> usize
    where
        Command: PartialEq,
    {
        self.mutate_command(command, |item| item.set_mark(mark))
    }

    /// Configures every matching command item as checked or unchecked.
    pub fn set_checked(&mut self, command: &Command, checked: bool) -> usize
    where
        Command: PartialEq,
    {
        self.set_mark(command, MenuItemMark::Checked(checked))
    }

    /// Configures every matching command item as a selected or unselected radio choice.
    pub fn set_radio(&mut self, command: &Command, selected: bool) -> usize
    where
        Command: PartialEq,
    {
        self.set_mark(command, MenuItemMark::Radio(selected))
    }

    /// Applies one state mutation to every item that carries `command`.
    fn mutate_command(&mut self, command: &Command, mut update: impl FnMut(&mut MenuItemSpec<Command>)) -> usize
    where
        Command: PartialEq,
    {
        let mut changed = 0;
        for menu in &mut self.specification.menus {
            for entry in &mut menu.entries {
                let Some(item) = entry.item_mut() else { continue };
                if item.command == *command {
                    update(item);
                    changed += 1;
                }
            }
        }
        if changed != 0 {
            self.synchronize_open_panel();
        }
        changed
    }

    /// Copies the active semantic menu into the independently retained popup panel.
    fn synchronize_open_panel(&mut self) {
        let Some(index) = self.active_menu else { return };
        let Some(menu) = self.specification.menus.get(index) else {
            self.finish_close();
            return;
        };
        if self
            .panel
            .try_update_with(menu.entries.clone(), |panel, entries| {
                panel.set_menu(entries, self.active_minimum_width);
            })
            .is_err()
        {
            panic!("window-menu panel must remain mounted");
        }
    }

    /// Applies one bar request at the borrow-safe application event boundary.
    fn bar_submitted(&mut self, context: &mut EventContext<'_>, event: &MenuBarSubmitted) {
        if !event.open {
            context
                .set_root_visible(self.popup.id(), false)
                .expect("window-menu popup must remain registered");
            self.finish_close();
            return;
        }

        let Some(menu) = self.specification.menus.get(event.index) else {
            // A stale event can only arise if future APIs replace a specification between retained
            // update and semantic dispatch. Closing is a safe deterministic fallback.
            self.finish_close();
            return;
        };
        if self
            .panel
            .try_update_with(menu.entries.clone(), |panel, entries| panel.set_menu(entries, event.anchor.width))
            .is_err()
        {
            panic!("window-menu panel must remain mounted");
        }

        // Opening and placement form one checked window-manager transaction. This prevents a
        // pointer-relative intermediate rectangle and guarantees popup replacement observes the new
        // menu's exact heading anchor.
        context
            .show_popup_at(self.popup.id(), event.anchor)
            .expect("window-menu popup must remain registered");
        self.active_menu = Some(event.index);
        self.active_minimum_width = event.anchor.width.max(0);
        self.bar
            .try_update(|bar| bar.set_open_menu(Some(event.index)))
            .expect("window-menu bar must remain mounted");
    }

    /// Closes the popup and publishes one selected application command.
    fn panel_submitted(&mut self, context: &mut EventContext<'_>, event: &MenuPanelSubmitted<Command>) {
        context
            .set_root_visible(self.popup.id(), false)
            .expect("window-menu popup must remain registered");
        self.finish_close();
        self.invoked_event.borrow_mut().emit(MenuInvoked { command: event.command.clone() });
    }

    /// Reconciles component state after generic popup dismissal or replacement.
    fn popup_submitted(&mut self, event: &RootSubmitted) {
        if matches!(event, RootSubmitted::PopupDismissed) {
            // WindowManager has already hidden the root. Only the component and persistent bar need
            // reconciliation, so no context-aware subscription or redundant root mutation is needed.
            self.finish_close();
        }
    }

    /// Clears every non-root representation of an open menu.
    fn finish_close(&mut self) {
        self.active_menu = None;
        self.active_minimum_width = 0;
        self.bar.try_update(|bar| bar.set_open_menu(None)).expect("window-menu bar must remain mounted");
    }

    /// Type-erased adapter for a top-level bar event subscription.
    fn dispatch_bar_submitted<State>(
        state: &mut State,
        accessor: &WindowMenuAccessor<State, Command>,
        context: &mut EventContext<'_>,
        event: &MenuBarSubmitted,
    ) {
        accessor(state).bar_submitted(context, event);
    }

    /// Type-erased adapter for a popup command-selection subscription.
    fn dispatch_panel_submitted<State>(
        state: &mut State,
        accessor: &WindowMenuAccessor<State, Command>,
        context: &mut EventContext<'_>,
        event: &MenuPanelSubmitted<Command>,
    ) {
        accessor(state).panel_submitted(context, event);
    }

    /// Type-erased adapter for generic popup dismissal notifications.
    fn dispatch_popup_submitted<State>(state: &mut State, accessor: &WindowMenuAccessor<State, Command>, event: &RootSubmitted) {
        accessor(state).popup_submitted(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{NoopRenderer, test_atlas};
    use crate::{Dimensioni, MouseButton, RootChrome, TextBlock, TextBlockParameters, rect};

    /// Small semantic command set proving that public menu delivery remains application typed.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum Command {
        New,
        Save,
        WordWrap,
        About,
    }

    /// Application state used by complete Context/component behavior tests.
    struct Model {
        /// Menu component found by the constructor's retained accessor.
        menu: WindowMenu<Command>,
        /// Commands observed through the public component-owned event port.
        invoked: Vec<Command>,
    }

    impl Model {
        /// Resolves the same component location for every internal menu subscription.
        fn menu_mut(state: &mut Self) -> &mut WindowMenu<Command> {
            &mut state.menu
        }

        /// Records one public command delivery for interaction assertions.
        fn command_invoked(&mut self, event: &MenuInvoked<Command>) {
            self.invoked.push(event.command);
        }
    }

    /// Returns a representative specification with groups, state, and shortcut presentation.
    fn specification() -> MenuBarSpec<Command> {
        MenuBarSpec::new([
            MenuSpec::new(
                "File",
                [
                    MenuEntry::item("New", Command::New).shortcut_hint("Ctrl+N"),
                    MenuEntry::item("Save", Command::Save).disabled(),
                ],
            ),
            MenuSpec::new(
                "View",
                [
                    MenuEntry::item("Word Wrap", Command::WordWrap).checked(true),
                    MenuEntry::separator(),
                    MenuEntry::item("About", Command::About),
                ],
            ),
        ])
    }

    /// Constructs a context and installs the returned menu at its promised accessor location.
    fn context_and_model() -> (Context<NoopRenderer, Model>, Model) {
        let mut context = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(480, 320));
        let body = TextBlock::create(TextBlockParameters::new("body")).1;
        let menu = WindowMenu::create(&mut context, Model::menu_mut, "Menu Window", rect(20, 20, 260, 180), specification(), body);
        let invoked = menu.invoked();
        context.subscribe(invoked, Model::command_invoked).unwrap();
        let model = Model { menu, invoked: Vec::new() };
        (context, model)
    }

    /// Clicks one screen-space point and commits press and release as distinct transactions.
    fn click(context: &mut Context<NoopRenderer, Model>, model: &mut Model, x: i32, y: i32) {
        context.mousemove(x, y);
        context.mousedown(x, y, MouseButton::LEFT);
        context.update_ui_state(Dimensioni::new(480, 320), model);
        context.mouseup(x, y, MouseButton::LEFT);
        context.update_ui_state(Dimensioni::new(480, 320), model);
    }

    #[test]
    fn specification_builders_preserve_groups_and_item_state() {
        let specification = specification();
        assert_eq!(specification.menus().len(), 2);
        assert_eq!(specification.menus()[0].label(), "File");
        assert_eq!(specification.menus()[1].entries().len(), 3);
        assert!(specification.menus()[1].entries()[1].is_separator());
        let save = specification.menus()[0].entries()[1].as_item().unwrap();
        assert!(!save.is_enabled());
        assert_eq!(save.command(), &Command::Save);
    }

    #[test]
    fn command_mutation_updates_all_matching_entries_and_open_snapshot() {
        let (mut context, mut model) = context_and_model();
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        let bar = context.debug_root_node_rect(model.menu.window.id(), model.menu.bar_node).unwrap();
        click(&mut context, &mut model, bar.x + 8, bar.y + bar.height / 2);
        assert!(!model.menu.panel.try_read(|panel| panel.entries()[1].as_item().unwrap().is_enabled()).unwrap());

        assert_eq!(model.menu.set_enabled(&Command::Save, true), 1);
        assert_eq!(model.menu.set_checked(&Command::WordWrap, false), 1);
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        assert!(model.menu.panel.try_read(|panel| panel.entries()[1].as_item().unwrap().is_enabled()).unwrap());
        let file = &model.menu.specification().menus()[0];
        assert!(file.entries()[1].as_item().unwrap().is_enabled());
        let view = &model.menu.specification().menus()[1];
        assert_eq!(view.entries()[0].as_item().unwrap().marker(), MenuItemMark::Checked(false));
    }

    #[test]
    fn enabled_panel_item_emits_typed_command_and_closes_popup() {
        let (mut context, mut model) = context_and_model();
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        let bar = context.debug_root_node_rect(model.menu.window.id(), model.menu.bar_node).unwrap();
        click(&mut context, &mut model, bar.x + 8, bar.y + bar.height / 2);

        let panel = context
            .debug_root_node_rect(model.menu.popup.id(), model.menu.panel_node)
            .expect("visible menu panel must have committed geometry");
        // The File menu has two equal-height item rows, so the first quarter of the complete panel
        // height is the center of its enabled first row.
        click(&mut context, &mut model, panel.x + panel.width / 2, panel.y + panel.height / 4);

        assert_eq!(model.invoked, [Command::New]);
        assert!(!model.menu.is_open());
        assert_eq!(model.menu.popup.widget().try_read(RootChrome::is_visible), Some(false));
    }

    #[test]
    fn popup_dismissal_reconciles_component_and_bar_state() {
        let (mut context, mut model) = context_and_model();
        context.update_ui_state(Dimensioni::new(480, 320), &mut model);
        let bar = context.debug_root_node_rect(model.menu.window.id(), model.menu.bar_node).unwrap();
        click(&mut context, &mut model, bar.x + 8, bar.y + bar.height / 2);
        assert!(model.menu.is_open());

        // A press outside both roots triggers ordinary popup dismissal before it falls through.
        click(&mut context, &mut model, 460, 300);
        assert!(!model.menu.is_open());
        assert_eq!(model.menu.bar.try_read(MenuBar::open_menu), Some(None));
        assert_eq!(model.menu.popup.widget().try_read(RootChrome::is_visible), Some(false));
    }
}
