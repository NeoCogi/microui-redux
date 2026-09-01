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

use std::{cell::RefCell, rc::Rc};

use crate::{
    AppearanceRole, AtlasHandle, ChildParticipation, Container, ContainerWidget, ControlColor, Dimensioni, MeasureCtx, Recti, Style, TypedWidgetHandle,
    UiInputEvent, VisualState, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

use super::{Children, ContainerLayoutCtx, LinearItem, Node};
use crate::{Linear, LinearParameters};

#[derive(Copy, Clone)]
enum DisclosureVariant {
    Header,
    Tree,
}

/// One-shot construction input for a stateful disclosure container.
///
/// Label, header/tree presentation, and base options are initialization-only. Initial expansion
/// and children move into [`Disclosure`] for mounted mutation.
pub struct DisclosureParameters {
    label: String,
    expanded: bool,
    children: Vec<LinearItem>,
    variant: DisclosureVariant,
    opt: WidgetOption,
}

impl WidgetParameters for DisclosureParameters {}

impl DisclosureParameters {
    /// Creates the framed header presentation and its top-to-bottom body items.
    ///
    /// Plain [`Node`] values use content height. Pass [`LinearItem`] when an item has an explicit
    /// fixed or flexible relationship to the private vertical Linear.
    pub fn header<T>(label: impl Into<String>, expanded: bool, children: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<LinearItem>,
    {
        Self {
            label: label.into(),
            expanded,
            children: children.into_iter().map(Into::into).collect(),
            variant: DisclosureVariant::Header,
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates the unframed, indented tree presentation and its body items.
    pub fn tree<T>(label: impl Into<String>, expanded: bool, children: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<LinearItem>,
    {
        Self {
            label: label.into(),
            expanded,
            children: children.into_iter().map(Into::into).collect(),
            variant: DisclosureVariant::Tree,
            opt: WidgetOption::NONE,
        }
    }

    /// Overrides the presentation's default widget options.
    pub fn with_options(mut self, opt: WidgetOption) -> Self {
        self.opt = opt;
        self
    }
}

/// Application-facing state for a disclosure container.
///
/// Expansion gates descendant traversal while retaining every owned runtime and its state. It does
/// not expose or mutate generic node visibility.
pub struct Disclosure {
    /// Weak access to content topology strongly owned by the vertical Linear body child.
    content: TypedWidgetHandle<Linear>,
    expanded: bool,
    variant: DisclosureVariant,
}

impl Disclosure {
    /// Returns whether descendants participate in traversal.
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Returns whether descendants are currently gated.
    pub fn is_collapsed(&self) -> bool {
        !self.expanded
    }

    /// Makes descendants participate in traversal.
    pub fn expand(&mut self) {
        self.expanded = true;
    }

    /// Gates descendants while retaining their ownership and state.
    pub fn collapse(&mut self) {
        self.expanded = false;
    }

    /// Reverses the current descendant gate.
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Returns the number of owned child nodes.
    pub fn len(&self) -> Option<usize> {
        self.content.try_read(Linear::len).flatten()
    }

    /// Returns whether the disclosure owns no children.
    pub fn is_empty(&self) -> Option<bool> {
        self.content.try_read(Linear::is_empty).flatten()
    }

    /// Appends one still-unmounted body item and its vertical Linear-owned height track.
    #[allow(clippy::result_large_err)] // Failure returns the exact unique node and its edge metadata.
    pub fn push(&mut self, item: impl Into<LinearItem>) -> Result<(), LinearItem> {
        self.content.try_update_with(item.into(), Linear::push)?
    }

    /// Inserts an item, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, item: LinearItem) -> Result<(), LinearItem> {
        self.content.try_update_with(item, |content, item| content.insert(index, item))?
    }

    /// Drops one indexed child owner and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.content.try_update(|content| content.remove_drop(index)).flatten()
    }

    /// Drops every child owner.
    pub fn clear(&mut self) -> Option<()> {
        self.content.try_update(Linear::clear).flatten()
    }

    /// Replaces all body items in iterator order.
    pub fn replace<T, I>(&mut self, items: I) -> Result<(), I>
    where
        T: Into<LinearItem>,
        I: IntoIterator<Item = T>,
    {
        self.content.try_update_with(items, Linear::replace)?
    }

    // Structural child order is also retained keyboard traversal order. Keeping the visible
    // header first makes Tab reach it before any expanded descendants without a parallel order.
    const HEADER: usize = 0;
    const BODY: usize = 1;

    fn indent(&self, style: &Style) -> i32 {
        if matches!(self.variant, DisclosureVariant::Tree) {
            style.indent.max(0)
        } else {
            0
        }
    }
}

/// Dispatcher-addressable disclosure header.
///
/// The header owns no descendants and keeps only a weak typed handle to its parent widget.
struct DisclosureHeader {
    disclosure: TypedWidgetHandle<Disclosure>,
    label: String,
    variant: DisclosureVariant,
    opt: WidgetOption,
}

impl DisclosureHeader {
    /// Measures the complete custom-painted header, including its optional internal frame.
    fn preferred(&self, style: &Style, atlas: &AtlasHandle) -> Dimensioni {
        // Resolve icon and text metrics independently, then build the one-row content preference.
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font_height = atlas.get_font_height(style.font) as i32;
        // Expansion changes only presentation state, not the header's retained allocation. Reserve
        // the component-wise maximum of both possible icons so toggling cannot clip a larger
        // collapse image or require a state-dependent measurement invalidation.
        let expand_icon = atlas.get_icon_size(style.icons.expand);
        let collapse_icon = atlas.get_icon_size(style.icons.collapse);
        let icon = Dimensioni::new(expand_icon.width.max(collapse_icon.width), expand_icon.height.max(collapse_icon.height));
        let text_width = if self.label.is_empty() {
            0
        } else {
            atlas.get_text_size(style.font, &self.label).width
        };
        let content_height = font_height.max(icon.height).max(0).saturating_add(vertical_pad.saturating_mul(2));
        let icon_width = content_height.saturating_sub(padding).max(icon.width).max(0);
        let content_width = padding.saturating_mul(2).saturating_add(icon_width).saturating_add(text_width.max(0));
        let content = Dimensioni::new(content_width, content_height);
        // A header frame is internal to this child, so preferred size must include its inset here.
        let frame = if self.opt.intersects(WidgetOption::FRAME) {
            style.appearance(AppearanceRole::Button, VisualState::Normal).insets.normalized()
        } else {
            crate::SliceInsets::ZERO
        };
        crate::ui_node::frame::outer_preferred(content, frame)
    }
}

impl Widget for DisclosureHeader {
    fn widget_opt(&self) -> &WidgetOption {
        // Static options describe only the header child, never the complete disclosure allocation.
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Pointer/Enter/Space toggle while directional keys expose the conventional tree behavior.
        match ctx.action(input, self.keyboard_behavior()) {
            Some(crate::KeyboardAction::Activate) => {
                let _ = self.disclosure.try_update(Disclosure::toggle);
            }
            Some(crate::KeyboardAction::Expand) => {
                let _ = self.disclosure.try_update(Disclosure::expand);
            }
            Some(crate::KeyboardAction::Collapse) => {
                let _ = self.disclosure.try_update(Disclosure::collapse);
            }
            Some(crate::KeyboardAction::Decrease | crate::KeyboardAction::Increase) | None => {}
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Expansion determines the icon, while presentation variant determines background/frame.
        let Some(expanded) = self.disclosure.try_read(Disclosure::is_expanded) else {
            return;
        };
        let mut row = ctx.local_rect();
        match self.variant {
            DisclosureVariant::Header => {
                if self.opt.intersects(WidgetOption::FRAME) {
                    // The header owns this internal frame, so it resolves the complete button patch
                    // rather than asking retained traversal to inset the disclosure container.
                    row = ctx.draw_appearance(AppearanceRole::Button, row).unwrap_or_default();
                } else {
                    ctx.draw_appearance_center(AppearanceRole::DisclosureHeader, row);
                }
            }
            DisclosureVariant::Tree => ctx.draw_appearance_center(AppearanceRole::DisclosureHeader, row),
        }

        // Reserve a square icon cell from row height, then paint text in the remaining rectangle.
        let text_color = ctx.control_color(ControlColor::Text);
        ctx.draw_icon(
            if expanded { ctx.style().icons.collapse } else { ctx.style().icons.expand },
            Recti::new(row.x, row.y, row.height, row.height),
            text_color,
        );
        // text_offset = row_height - padding.
        let offset = row.height.saturating_sub(ctx.style().padding);
        // text_x = row_x + text_offset; text_width = row_width - text_offset.
        let text_rect = Recti::new(row.x.saturating_add(offset), row.y, row.width.saturating_sub(offset), row.height);
        ctx.draw_control_text_with_font(ctx.style().font, &self.label, text_rect, ControlColor::Text, self.opt);
    }

    // Framing belongs to the header sub-rectangle, not the complete descendant allocation.
    fn effective_widget_opt(&self) -> WidgetOption {
        self.opt & !WidgetOption::FRAME
    }

    fn keyboard_behavior(&self) -> crate::KeyboardBehavior {
        // The addressable header, rather than the structural disclosure container, participates in
        // sequential focus and later shared activation.
        crate::KeyboardBehavior::TAB_STOP
            | crate::KeyboardBehavior::ACTIVATE_ENTER
            | crate::KeyboardBehavior::ACTIVATE_SPACE
            | crate::KeyboardBehavior::EXPAND_COLLAPSE
    }
}

impl crate::LeafWidget for DisclosureHeader {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _constraints: crate::Constraints) -> Dimensioni {
        // Header content is intrinsically one line and does not stretch to the offered bound.
        self.preferred(style, atlas)
    }
}

impl ContainerWidget for Disclosure {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: crate::Constraints) -> Dimensioni {
        // The header always contributes. Body measurement is conditional so collapsed content does
        // not influence root auto-size while its state and nodes remain retained.
        let header = ctx.measure_child(Self::HEADER, constraints).unwrap_or_default();
        if !self.expanded {
            return header;
        }
        let indent = self.indent(ctx.style());
        let spacing = ctx.style().spacing.max(0);
        let body_constraints = crate::Constraints::new(
            constraints.width.shrink(indent),
            constraints.height.shrink(header.height.saturating_add(spacing)),
        );
        let body = ctx.measure_child(Self::BODY, body_constraints).unwrap_or_default();
        Dimensioni::new(
            header.width.max(body.width.saturating_add(indent)),
            header.height.saturating_add(spacing).saturating_add(body.height),
        )
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Header is a fixed structural role and always occupies the first visible row.
        let preferred = ctx
            .measure_child(children, Self::HEADER, crate::Constraints::bounded(Dimensioni::new(rect.width, rect.height)))
            .unwrap_or_default();
        let header_height = preferred.height.min(rect.height.max(0));
        let _ = ctx.set_child_participation(children, Self::HEADER, ChildParticipation::Active);
        let _ = ctx.layout_child(children, Self::HEADER, Recti::new(rect.x, rect.y, rect.width, header_height));

        // Participation is the single gate consumed by update, paint, hit testing, and target
        // sanitation. No generic visibility bit or topology rewrite is needed for collapse.
        let participation = if self.expanded {
            ChildParticipation::Active
        } else {
            ChildParticipation::Hidden
        };
        let _ = ctx.set_child_participation(children, Self::BODY, participation);
        if self.expanded {
            let indent = self.indent(ctx.style());
            let spacing = ctx.style().spacing.max(0);
            // body_origin = container_origin + (indent, header_height + spacing).
            let body_x = rect.x.saturating_add(indent);
            let body_y = rect.y.saturating_add(header_height).saturating_add(spacing);
            // body_extent = container_extent - (indent, header_height + spacing).
            let body_width = rect.width.saturating_sub(indent);
            let body_height = rect.height.saturating_sub(header_height).saturating_sub(spacing);
            let body = Recti::new(body_x, body_y, body_width, body_height);
            let _ = ctx.layout_child(children, Self::BODY, body);
        }
    }
}

impl Widget for Disclosure {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

/// Builds the fixed body/header structure before wrapping it in the public owning node.
fn create_container(parameters: DisclosureParameters) -> (TypedWidgetHandle<Disclosure>, Container) {
    let (content, body) = Linear::create(LinearParameters::vertical(parameters.children));
    let widget = Rc::new(RefCell::new(crate::ui_node::WidgetStorage::new(Disclosure {
        content,
        expanded: parameters.expanded,
        variant: parameters.variant,
    })));
    let handle = TypedWidgetHandle::new(&widget);
    let header = DisclosureHeader {
        disclosure: handle.clone(),
        label: parameters.label,
        variant: parameters.variant,
        opt: parameters.opt,
    };
    // Retained child order mirrors visual order so hit testing, paint, and Tab traversal share one
    // authoritative topology. Role constants keep layout independent from representation details.
    let children = Rc::new(RefCell::new([Node::widget_internal(header), body].into_iter().collect()));
    let (_, container) = Container::from_shared_owner(children, widget);
    (handle, container)
}

impl Disclosure {
    /// Creates a child-owning disclosure and returns its weak typed widget handle plus completed node.
    ///
    /// Construction is complete before the node is returned: callers never observe or manipulate
    /// the two structural children separately.
    pub fn create(parameters: DisclosureParameters) -> (TypedWidgetHandle<Disclosure>, Node) {
        // Add the common Node runtime state around the concrete Container ownership boundary.
        let (state, container) = create_container(parameters);
        (state, Node::container(container))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructs one valid theme atlas whose expanded-state icon is larger than the collapsed one.
    fn asymmetric_disclosure_atlas() -> AtlasHandle {
        // Every texel is opaque white so overlapping semantic rectangles remain valid; only the
        // declared dimensions matter to this measurement regression.
        let pixels = vec![0xFF; 10 * 5 * 4];
        let icons = [
            ("white", Recti::new(0, 0, 1, 1)),
            ("close", Recti::new(0, 0, 1, 1)),
            ("expand", Recti::new(0, 0, 1, 1)),
            ("collapse", Recti::new(0, 0, 10, 5)),
            ("check", Recti::new(0, 0, 1, 1)),
            ("expand_down", Recti::new(0, 0, 1, 1)),
            ("open_folder", Recti::new(0, 0, 1, 1)),
            ("closed_folder", Recti::new(0, 0, 1, 1)),
            ("file", Recti::new(0, 0, 1, 1)),
        ];
        let glyphs = [(
            '_',
            crate::CharEntry {
                offset: crate::Vec2i::new(0, 0),
                advance: crate::Vec2i::new(1, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "body",
            crate::FontEntry {
                line_size: 1,
                baseline: 1,
                font_size: 1,
                entries: &glyphs,
            },
        )];
        let source = crate::AtlasSource {
            width: 10,
            height: 5,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: crate::SourceFormat::Raw,
        };

        AtlasHandle::try_from(&source).expect("asymmetric disclosure fixture must satisfy the atlas contract")
    }

    #[test]
    fn header_and_tree_preserve_state_and_fixed_structural_children() {
        let (header_state, header) = create_container(DisclosureParameters::header("Header", false, std::iter::empty::<LinearItem>()));
        assert_eq!(header_state.try_read(Disclosure::is_collapsed), Some(true));
        header_state.try_update(Disclosure::toggle).unwrap();
        assert_eq!(header_state.try_read(Disclosure::is_expanded), Some(true));
        assert_eq!(Node::container(header).debug_node_count(), 3);

        let custom_opt = WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT;
        let (tree_state, tree) = create_container(DisclosureParameters::tree("Tree", true, std::iter::empty::<LinearItem>()).with_options(custom_opt));
        assert_eq!(tree_state.try_read(Disclosure::is_expanded), Some(true));
        assert_eq!(Node::container(tree).debug_node_count(), 3);
    }

    /// Verifies either expansion state fits the one state-independent header allocation.
    #[test]
    fn header_measurement_reserves_the_larger_expand_or_collapse_icon() {
        let atlas = asymmetric_disclosure_atlas();
        let style = Style { padding: 0, ..Style::from_atlas(&atlas) };
        let (_, container) = create_container(DisclosureParameters::tree("", false, std::iter::empty::<LinearItem>()));
        let mut root = Node::container(container);
        let mut runtime = crate::ui_node::UiRuntime::new();

        let preferred = runtime.measure_tree_root(&mut root, &style, &atlas, crate::Constraints::unbounded());

        // The 10x5 collapse icon plus one vertical pixel on each side determines 10x7. Measuring
        // only the 1x1 expand icon would incorrectly return 3x3 and clip after expansion.
        assert_eq!((preferred.width, preferred.height), (10, 7));
    }
}
