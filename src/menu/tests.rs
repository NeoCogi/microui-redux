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

//! Structural and geometry regressions for the compact menu compiler.

use super::*;

use crate::test_support::test_atlas;
use crate::ui_node::text_layout::control_text_position_with_font;

/// Proves that an explicitly empty bar cannot reserve a blank interactive strip.
#[test]
fn empty_bar_has_zero_geometry() {
    // All later phases consume this same cached layout, so zero measurement also means no slot can
    // paint, hit, or anchor a popup even if the surrounding linear shell stretches horizontally.
    let layout = layout_bar(&[], &Style::default(), &test_atlas());
    assert_eq!((layout.size.width, layout.size.height), (0, 0));
    assert!(layout.slots.is_empty());
    assert_eq!(layout.marker_width, 0);
}

/// Proves that compilation retains only the bar shell and one leaf per logical popup.
#[test]
fn nested_declarations_compile_parent_first_without_row_nodes_or_strong_handles() {
    // Keep weak handles to items at two hierarchy depths so the final lifetime assertion covers
    // both nested and top-level popup ownership.
    let (deep_handle, deep_item) = MenuItem::create(MenuItemParameters::new("Deep action"));
    let (edit_handle, edit_item) = MenuItem::create(MenuItemParameters::new("Edit action"));
    let menu = MenuBar::new([
        Menu::new("File").submenu(Menu::new("Recent").submenu(Menu::new("Deep").item(deep_item))),
        Menu::new("Edit").item(edit_item),
    ]);

    // A one-node placeholder makes the expected bar/body shell budget exact: the vertical shell,
    // the compact bar leaf, and the application body leaf.
    let body = Node::widget(WidgetOption::NONE);
    let (shell, popups, controller) = menu.compile("Topology", body);

    // Recursive compilation reserves each parent before visiting its children, then resumes with
    // the next top-level heading. These indices are the manager's stable popup-path vocabulary.
    let parents: Vec<_> = popups.iter().map(|popup| popup.parent).collect();
    assert_eq!(parents, [None, Some(0), Some(1), None]);
    assert_eq!(popups.len(), 4, "File, Recent, Deep, and Edit each require one popup");

    // Logical rows live as values inside each MenuSurface, so every popup content tree is exactly
    // one retained node. The test-only controller identities must name those same four leaves.
    for (menu_id, popup) in popups.iter().enumerate() {
        assert_eq!(popup.content.debug_node_count(), 1, "popup {menu_id} unexpectedly retained row nodes");
        assert_eq!(controller.popup_node(menu_id), Some(popup.content.id()));
    }
    assert_eq!(shell.debug_node_count(), 3, "the bar/body shell must remain a three-node tree");
    assert_eq!(shell.debug_node_count() + popups.len(), 7, "the complete declaration must retain seven nodes");
    assert!(deep_handle.is_alive() && edit_handle.is_alive(), "compiled popup leaves own their item state");

    // Dropping every compiled output removes all strong owners. Application handles and event
    // endpoints are deliberately weak, so neither can extend a destroyed menu's lifetime.
    drop((shell, popups, controller));
    assert!(!deep_handle.is_alive() && !edit_handle.is_alive());
}

/// Proves that the shared popup text region reserves unclipped, non-overlapping trailing content.
#[test]
fn popup_text_region_separates_labels_from_shortcuts_and_submenu_arrows() {
    // Use deterministic test-atlas glyph advances and include both trailing-content variants in
    // one popup so production layout must choose shared label and trailing maxima.
    let atlas = test_atlas();
    let style = Style::default();
    let (_, item) = MenuItem::create(MenuItemParameters::new("aaaa").shortcut_hint("bbbb"));
    let rows = vec![MenuSlot::Item(item.state), MenuSlot::Branch { label: "aaaaaa".into(), target: 1 }];
    let layout = layout_popup(&rows, &style, &atlas);
    let padding = style.padding.max(1);

    // The item label and right-aligned shortcut are positioned by the same production helper and
    // clipped by the same production region. Their measured extents must fit without intersecting.
    let item_region = text_region(layout.slots[0], layout.marker_width);
    let item_font = style.resolve_font_choice(FontChoice::Role(FontRole::Body));
    let label_size = atlas.get_text_size(item_font, "aaaa");
    let hint_size = atlas.get_text_size(item_font, "bbbb");
    let label_position = control_text_position_with_font(&style, &atlas, item_font, "aaaa", item_region, WidgetOption::NONE);
    let hint_position = control_text_position_with_font(&style, &atlas, item_font, "bbbb", item_region, WidgetOption::ALIGN_RIGHT);
    let item_right = item_region.x + item_region.width;
    assert!(label_position.x >= item_region.x && label_position.x + label_size.width <= item_right);
    assert!(hint_position.x >= item_region.x && hint_position.x + hint_size.width <= item_right);
    assert!(label_position.x + label_size.width <= hint_position.x, "label and shortcut must not overlap");

    // A submenu label uses the identical horizontal region while its arrow uses the production
    // trailing rectangle. Both must stay inside the row, with at least the intended padding gap.
    let branch_row = layout.slots[1];
    let branch_region = text_region(branch_row, layout.marker_width);
    let branch_size = atlas.get_text_size(item_font, "aaaaaa");
    let branch_position = control_text_position_with_font(&style, &atlas, item_font, "aaaaaa", branch_region, WidgetOption::NONE);
    let arrow_size = atlas.get_icon_size(style.icons.expand);
    let arrow = trailing_rect(branch_region, arrow_size, padding);
    let branch_right = branch_position.x + branch_size.width;
    assert_eq!((item_region.x, item_region.width), (branch_region.x, branch_region.width));
    assert!(branch_right.saturating_add(padding) <= arrow.x, "submenu label and arrow must retain their gap");
    assert!(arrow.x >= branch_region.x && arrow.x + arrow.width <= branch_region.x + branch_region.width);
    assert!(arrow.y >= branch_row.y && arrow.y + arrow.height <= branch_row.y + branch_row.height);
}
