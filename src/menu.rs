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

//! Declarative application menus owned by a retained window.
//!
//! [`MenuBar`] and [`Menu`] describe unique retained nodes once, at window construction. Compilation
//! mounts the persistent bar above the window body and produces parent-before-child popup records for
//! the window manager. Heading and submenu rows publish no private events: their retained node IDs are
//! the direct interaction and placement anchors used by the manager. Application commands remain
//! ordinary concrete [`MenuItem`] widgets with independently subscribable typed submission ports.

use crate::ui_node::RuntimeNodeId;
use crate::ui_node::widgets::{MenuBarSurface, MenuHeading, MenuList, MenuSeparator, MenuSubmenu};
use crate::{Linear, LinearItem, LinearParameters, Node, TypedWidgetHandle};

pub use crate::ui_node::widgets::{MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted};

/// Complete declarative menu bar installed intrinsically into one window.
pub struct MenuBar {
    /// Top-level menus retained in their left-to-right heading order.
    menus: Vec<Menu>,
}

impl MenuBar {
    /// Creates a bar from uniquely owned top-level menu descriptions.
    pub fn new(menus: impl IntoIterator<Item = Menu>) -> Self {
        // Collect at the public ownership boundary so compilation can consume every menu exactly once.
        Self { menus: menus.into_iter().collect() }
    }

    /// Compiles this declaration into the window's persistent content and directly owned popups.
    pub(crate) fn compile(self, window_name: &str, content: Node) -> CompiledMenuBar {
        // Build each concrete heading before moving its node into the bar. Its process-unique node ID
        // then remains the stable interaction and placement relationship for that menu's popup.
        let mut headings = Vec::with_capacity(self.menus.len());
        let mut popups = Vec::new();
        for menu in self.menus {
            let heading_label = menu.label.clone();
            let (heading, heading_node) = MenuHeading::create(heading_label);
            let anchor = MenuAnchor::Below { node: heading_node.id(), heading };
            compile_popup(menu, None, None, window_name, anchor, &mut popups);
            headings.push(heading_node);
        }

        // The private bar surface paints the complete row, while existing Linear ownership places it
        // before a flexible body without introducing a second window or application coordinator.
        let bar = MenuBarSurface::create(headings);
        let (_, shell) = Linear::create(LinearParameters::vertical([LinearItem::content(bar), LinearItem::flex(content, 1.0)]));
        CompiledMenuBar { content: shell, popups }
    }
}

/// One top-level menu or recursively nested submenu.
pub struct Menu {
    /// User-visible heading or submenu-row label.
    label: String,
    /// Ordered retained rows transferred into this menu's popup during compilation.
    entries: Vec<MenuEntry>,
}

impl Menu {
    /// Creates an empty menu with the supplied user-visible label.
    pub fn new(label: impl Into<String>) -> Self {
        // Entries are appended explicitly so separators and submenus need no grouping wrapper.
        Self { label: label.into(), entries: Vec::new() }
    }

    /// Appends one uniquely owned actionable retained node.
    pub fn item(mut self, item: Node) -> Self {
        // Moving the node into the declaration preserves its single strong retained owner.
        self.entries.push(MenuEntry::Item(item));
        self
    }

    /// Appends one explicit non-interactive separator row.
    pub fn separator(mut self) -> Self {
        // Preserve the caller's exact order; compilation performs no implicit grouping or cleanup.
        self.entries.push(MenuEntry::Separator);
        self
    }

    /// Appends one recursively composed submenu.
    pub fn submenu(mut self, submenu: Menu) -> Self {
        // The nested value owns its entries until the recursive compiler transfers them to a child popup.
        self.entries.push(MenuEntry::Submenu(submenu));
        self
    }

    /// Splits this consumed description into the values needed to build one popup.
    fn into_parts(self) -> (String, Vec<MenuEntry>) {
        // Destructure once so neither the label nor any unique node needs to be cloned.
        (self.label, self.entries)
    }
}

/// One ordered row in a declarative menu.
enum MenuEntry {
    /// Application-authored actionable retained node.
    Item(Node),
    /// Explicit visual rule between caller-selected entries.
    Separator,
    /// Recursively owned child menu opened from a private submenu row.
    Submenu(Menu),
}

/// Persistent window content and popup definitions produced from one menu bar.
pub(crate) struct CompiledMenuBar {
    /// Private menu-bar surface followed by the application body in one vertical retained tree.
    pub(crate) content: Node,
    /// Top-level and nested popup definitions in parent-before-child order.
    pub(crate) popups: Vec<CompiledMenuPopup>,
}

/// One menu popup ready to be transferred directly into its window owner.
pub(crate) struct CompiledMenuPopup {
    /// Index of the direct parent popup, or `None` for a top-level window menu.
    pub(crate) parent: Option<usize>,
    /// Complete diagnostic name derived from the window and recursive menu-label path.
    pub(crate) name: String,
    /// Retained row relationship used for both pointer activation and live placement.
    pub(crate) anchor: MenuAnchor,
    /// Complete zero-gap menu surface uniquely owned by this definition.
    pub(crate) content: Node,
}

/// Retained node relationship that opens and positions one menu popup.
pub(crate) enum MenuAnchor {
    /// Top-level popup positioned below a persistent window-bar heading.
    Below {
        /// Stable retained identity used for direct route matching and rectangle lookup.
        node: RuntimeNodeId,
        /// Weak presentation access whose `open` bit is derived from the active popup path.
        heading: TypedWidgetHandle<MenuHeading>,
    },
    /// Child popup positioned to the right of a row in its parent popup.
    Right {
        /// Stable retained identity used for direct route matching and rectangle lookup.
        node: RuntimeNodeId,
        /// Weak presentation access whose `open` bit is derived from the active popup path.
        submenu: TypedWidgetHandle<MenuSubmenu>,
    },
}

impl MenuAnchor {
    /// Returns the retained node identity used by manager input and layout.
    pub(crate) const fn node(&self) -> RuntimeNodeId {
        // Both variants preserve the ID captured immediately before their node entered retained ownership.
        match self {
            Self::Below { node, .. } | Self::Right { node, .. } => *node,
        }
    }

    /// Returns whether this relationship places its popup below a window-bar heading.
    pub(crate) const fn is_below(&self) -> bool {
        // A boolean is sufficient because the only alternative is the submenu's right edge.
        matches!(self, Self::Below { .. })
    }

    /// Reconciles the trigger's derived paint state with manager-owned popup visibility.
    pub(crate) fn set_open(&self, open: bool) {
        // Open state affects paint only, so use the no-measurement path and keep the active popup path
        // as the sole semantic source of truth. Private menu topology guarantees each weak handle lives
        // exactly as long as the popup definition that refers to it.
        let updated = match self {
            Self::Below { heading, .. } => heading.try_update_without_measurement(|heading| heading.set_open(open)),
            Self::Right { submenu, .. } => submenu.try_update_without_measurement(|submenu| submenu.set_open(open)),
        };
        debug_assert!(updated.is_some(), "a retained menu anchor must outlive its popup definition");
    }
}

/// Recursively compiles one menu after its trigger node has already been constructed.
fn compile_popup(menu: Menu, parent: Option<usize>, parent_path: Option<&str>, window_name: &str, anchor: MenuAnchor, popups: &mut Vec<CompiledMenuPopup>) {
    // Resolve this diagnostic path before the menu label moves into any retained presentation widget.
    let (label, entries) = menu.into_parts();
    let path = parent_path.map_or_else(|| label.clone(), |parent_path| format!("{parent_path} {label}"));

    // Create child trigger rows while constructing the current surface, but retain their descriptions
    // locally until this parent record has been appended. Recursing afterward guarantees stable
    // parent-before-child indices without reserved slots or an intermediate popup tree.
    let index = popups.len();
    let mut rows = Vec::with_capacity(entries.len());
    let mut children = Vec::new();
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => rows.push(item),
            MenuEntry::Separator => rows.push(MenuSeparator::create()),
            MenuEntry::Submenu(submenu) => {
                let (handle, row) = MenuSubmenu::create(submenu.label.clone());
                let child_anchor = MenuAnchor::Right { node: row.id(), submenu: handle };
                rows.push(row);
                children.push((submenu, child_anchor));
            }
        }
    }
    popups.push(CompiledMenuPopup {
        parent,
        name: format!("{window_name} {path} Menu"),
        anchor,
        content: MenuList::create(rows),
    });

    // Compile complete child subtrees in row order. Each child names the just-appended record as its
    // direct parent, while recursive descendants receive their own immediately preceding parent.
    for (submenu, child_anchor) in children {
        compile_popup(submenu, Some(index), Some(&path), window_name, child_anchor, popups);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TextBlock, TextBlockParameters, Widget, WidgetOption};

    /// Creates one ordinary retained text node for compiler ownership tests.
    fn text(label: &str) -> Node {
        // TextBlock supplies a small uniquely owned leaf without introducing event-dispatch behavior.
        TextBlock::create(TextBlockParameters::new(label)).1
    }

    #[test]
    fn compiler_mounts_the_bar_and_flattens_popup_ancestry_parent_first() {
        // Retain handles only for ownership assertions; their nodes move once through the declaration.
        let (first, first_node) = MenuItem::create(MenuItemParameters::new("Open"));
        let (recent, recent_node) = MenuItem::create(MenuItemParameters::new("Recent Project"));
        let (help, help_node) = MenuItem::create(MenuItemParameters::new("About"));
        let declaration = MenuBar::new([
            Menu::new("File").item(first_node).separator().submenu(Menu::new("Recent").item(recent_node)),
            Menu::new("Help").item(help_node),
        ]);

        let compiled = declaration.compile("Editor", text("body"));

        // The shell contains itself, the bar surface, two headings, and the caller body.
        assert_eq!(compiled.content.debug_node_count(), 5);
        assert_eq!(compiled.popups.len(), 3);
        assert_eq!(compiled.popups[0].parent, None);
        assert_eq!(compiled.popups[1].parent, Some(0));
        assert_eq!(compiled.popups[2].parent, None);
        assert_eq!(compiled.popups[0].name, "Editor File Menu");
        assert_eq!(compiled.popups[1].name, "Editor File Recent Menu");
        assert_eq!(compiled.popups[2].name, "Editor Help Menu");
        assert!(compiled.popups[0].anchor.is_below());
        assert!(!compiled.popups[1].anchor.is_below());
        assert_ne!(compiled.popups[0].anchor.node(), compiled.popups[1].anchor.node());
        let MenuAnchor::Right { submenu, .. } = &compiled.popups[1].anchor else {
            panic!("a nested menu must compile to a right-of-row relationship");
        };
        assert_eq!(submenu.try_read(MenuSubmenu::is_open), Some(false));
        compiled.popups[1].anchor.set_open(true);
        assert_eq!(submenu.try_read(MenuSubmenu::is_open), Some(true));

        // An explicit separator and submenu trigger are concrete rows, never inferred group metadata.
        assert_eq!(compiled.popups[0].content.debug_node_count(), 4);
        assert_eq!(compiled.popups[1].content.debug_node_count(), 2);
        assert!(first.is_alive());
        assert!(recent.is_alive());
        assert!(help.is_alive());
    }

    #[test]
    fn compiled_owners_control_item_lifetime_and_anchor_highlight_is_derived() {
        // Keep weak handles to both an application item and the private top-level trigger.
        let (item, item_node) = MenuItem::create(MenuItemParameters::new("Run").disabled());
        let compiled = MenuBar::new([Menu::new("Tools").item(item_node)]).compile("Editor", text("body"));
        let MenuAnchor::Below { heading, .. } = &compiled.popups[0].anchor else {
            panic!("a top-level menu must compile to a below-heading relationship");
        };

        assert_eq!(heading.try_read(MenuHeading::is_open), Some(false));
        compiled.popups[0].anchor.set_open(true);
        assert_eq!(heading.try_read(MenuHeading::is_open), Some(true));
        assert!(item.is_alive());
        assert_eq!(item.is_enabled(), Some(false));
        assert_eq!(item.try_read(|item| item.widget_opt().intersects(WidgetOption::NO_INTERACT)), Some(true));
        assert_eq!(item.set_enabled(true), Some(()));
        assert_eq!(item.try_read(|item| item.widget_opt().intersects(WidgetOption::NO_INTERACT)), Some(false));

        // Dropping the compiled window content and popups releases every sole strong node owner.
        drop(compiled);
        assert!(!item.is_alive());
    }
}
