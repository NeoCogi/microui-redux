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
    let layout = layout_bar(&[], &Style::default(), &test_atlas(), Vec::new());
    assert_eq!((layout.size.width, layout.size.height), (0, 0));
    assert!(layout.slots.is_empty());
    assert_eq!(layout.marker_width, 0);
}

/// Proves that compilation retains recursive concrete surfaces without row nodes or identity maps.
#[test]
fn nested_declarations_compile_as_recursive_concrete_surfaces() {
    // Keep weak handles to items at two hierarchy depths so the final lifetime assertion covers
    // both nested and top-level popup ownership.
    let (deep_handle, deep_item) = MenuItem::create(MenuItemParameters::new("Deep action"));
    let (edit_handle, edit_item) = MenuItem::create(MenuItemParameters::new("Edit action"));
    let menu = MenuBar::new([
        Menu::new("File").submenu(Menu::new("Recent").submenu(Menu::new("Deep").item(deep_item))),
        Menu::new("Edit").item(edit_item),
    ]);

    let compiled = menu.compile();

    // The bar has two headings. File owns Recent, Recent owns Deep, and Edit is a sibling; no flat
    // vector or parent index is required to recover this declaration topology.
    assert_eq!(compiled.bar.rows.len(), 2);
    assert_eq!(compiled.popups.len(), 2);
    assert_eq!(compiled.popups[0].trigger_slot, 0);
    assert_eq!(compiled.popups[0].children.len(), 1);
    assert_eq!(compiled.popups[0].children[0].children.len(), 1);
    assert_eq!(compiled.popups[0].children[0].children[0].surface.rows.len(), 1);
    assert_eq!(compiled.popups[1].trigger_slot, 1);
    assert_eq!(compiled.popups[1].surface.rows.len(), 1);
    assert!(deep_handle.is_alive() && edit_handle.is_alive(), "compiled surfaces own their item records");

    // Dropping every compiled output removes all strong owners. Application handles and event
    // endpoints are deliberately weak, so neither can extend a destroyed menu's lifetime.
    drop(compiled);
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
    let rows = vec![MenuSlot::Item(item.record), MenuSlot::Branch { label: "aaaaaa".into() }];
    let layout = layout_popup(&rows, &style, &atlas, Vec::new());
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
