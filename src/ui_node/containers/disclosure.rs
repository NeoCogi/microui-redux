use std::{cell::RefCell, rc::Rc};

use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, COLLAPSE_ICON, ControlColor, Dimensioni, EXPAND_ICON, MouseButton, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx,
    WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};

use super::{
    Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerInputCtx, ContainerInputResult, ContainerLayoutCtx, ContainerState,
    Node,
};

#[derive(Copy, Clone)]
enum DisclosureVariant {
    Header,
    Tree,
}

/// One-shot construction input for a stateful disclosure container.
///
/// Label, header/tree presentation, and base options are initialization-only. Initial expansion
/// and children move into [`DisclosureState`] for mounted mutation.
pub struct DisclosureParameters {
    label: String,
    expanded: bool,
    children: Children,
    variant: DisclosureVariant,
    opt: WidgetOption,
}

impl WidgetParameters for DisclosureParameters {}

impl DisclosureParameters {
    /// Creates the framed header presentation.
    pub fn header(label: impl Into<String>, expanded: bool, children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            label: label.into(),
            expanded,
            children: children.into_iter().collect(),
            variant: DisclosureVariant::Header,
            opt: WidgetOption::FRAME,
        }
    }

    /// Creates the unframed, indented tree presentation.
    pub fn tree(label: impl Into<String>, expanded: bool, children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            label: label.into(),
            expanded,
            children: children.into_iter().collect(),
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
pub struct DisclosureState {
    children: Children,
    expanded: bool,
}

impl WidgetState for DisclosureState {}
impl ContainerState for DisclosureState {}

impl DisclosureState {
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
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Returns whether the disclosure owns no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Appends one still-unmounted child.
    pub fn push(&mut self, node: Node) {
        self.children.push(node);
    }

    /// Inserts a node, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.children.insert(index, node)
    }

    /// Drops one indexed child owner and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> bool {
        self.children.remove_drop(index)
    }

    /// Drops every child owner.
    pub fn clear(&mut self) {
        self.children.clear();
    }

    /// Replaces all descendants in iterator order.
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
        self.children.replace(nodes);
    }
}

/// Concrete state-owning disclosure runtime.
pub struct DisclosureContainer {
    state: Rc<RefCell<DisclosureState>>,
    label: String,
    variant: DisclosureVariant,
    opt: WidgetOption,
    /// Derived header geometry in this container's local content coordinates.
    header_rect: Recti,
    /// Routing-derived hover state for the header sub-rectangle.
    header_hovered: bool,
}

impl DisclosureContainer {
    fn header_preferred(&self, style: &Style, atlas: &AtlasHandle) -> Dimensioni {
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font_height = atlas.get_font_height(style.font) as i32;
        let icon = atlas.get_icon_size(EXPAND_ICON);
        let text_width = if self.label.is_empty() {
            0
        } else {
            atlas.get_text_size(style.font, &self.label).width
        };
        let content_height = (font_height.max(icon.height) + vertical_pad * 2).max(0);
        let icon_width = (content_height - padding).max(icon.width);
        let content = Dimensioni::new((padding * 2 + icon_width + text_width).max(0), content_height);
        let border = if self.opt.intersects(WidgetOption::FRAME) {
            style.frame_border().width
        } else {
            0
        };
        crate::frame::outer_preferred(content, border)
    }

    fn indent(&self, style: &Style) -> i32 {
        if matches!(self.variant, DisclosureVariant::Tree) {
            style.indent.max(0)
        } else {
            0
        }
    }
}

impl Widget for DisclosureContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        let header = self.header_preferred(style, atlas);
        runtime_read_state(&self.state, "Disclosure::measure", |state| {
            if !state.expanded {
                return Dimensioni::new(available.width.max(header.width), header.height);
            }

            let indent = self.indent(style);
            let child_available = Dimensioni::new(available.width.saturating_sub(indent), available.height.saturating_sub(header.height));
            let mut width = 0;
            let mut height: i32 = 0;
            for index in 0..state.children.len() {
                let child = state.children.measure_child(index, style, atlas, child_available).unwrap_or_default();
                width = width.max(child.width);
                height = height.saturating_add(child.height);
                if index + 1 < state.children.len() {
                    height = height.saturating_add(style.spacing);
                }
            }
            Dimensioni::new(
                available.width.max(header.width).max(width.saturating_add(indent)),
                header.height.saturating_add(style.spacing).saturating_add(height),
            )
        })
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let submitted = matches!(input, Some(UiInputEvent::MouseDown { button, .. }) if button.intersects(MouseButton::LEFT));
        if !submitted {
            return;
        }
        runtime_update_state(&self.state, "Disclosure::update", DisclosureState::toggle);
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let expanded = runtime_read_state(&self.state, "Disclosure::paint", DisclosureState::is_expanded);
        let mut row = self.header_rect;
        match self.variant {
            DisclosureVariant::Header => {
                let mut color = ControlColor::Button;
                if self.header_hovered {
                    color.hover();
                }
                if self.opt.intersects(WidgetOption::FRAME) {
                    row = ctx.draw_internal_frame(row, color).unwrap_or_default();
                } else {
                    ctx.draw_rect(row, ctx.style().colors[color as usize]);
                }
            }
            DisclosureVariant::Tree if self.header_hovered => {
                ctx.draw_rect(row, ctx.style().colors[ControlColor::ButtonHover as usize]);
            }
            DisclosureVariant::Tree => {}
        }

        let text_color = ctx.style().colors[ControlColor::Text as usize];
        ctx.draw_icon(
            if expanded { COLLAPSE_ICON } else { EXPAND_ICON },
            Recti::new(row.x, row.y, row.height, row.height),
            text_color,
        );
        let offset = row.height.saturating_sub(ctx.style().padding);
        let text_rect = Recti::new(row.x.saturating_add(offset), row.y, row.width.saturating_sub(offset), row.height);
        ctx.draw_control_text_with_font(ctx.style().font, &self.label, text_rect, ControlColor::Text, self.opt);
    }

    // Framing belongs to the header sub-rectangle, not the complete descendant allocation.
    fn effective_widget_opt(&self) -> WidgetOption {
        self.opt & !WidgetOption::FRAME
    }

    fn focus_policy(&self) -> crate::FocusPolicy {
        crate::FocusPolicy::from_widget_options(self.opt)
    }
}

impl WidgetStateOwner for DisclosureContainer {
    type State = DisclosureState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for DisclosureContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "Disclosure::visit_children", |state| {
            visitor.visit(&state.children);
        });
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "Disclosure::visit_children_mut", |state| {
            visitor.visit(&mut state.children);
        });
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        let header_height = self.header_preferred(ctx.style(), ctx.atlas()).height.min(rect.height.max(0));
        self.header_rect = Recti::new(rect.x, rect.y, rect.width, header_height);
        let expanded = runtime_read_state(&self.state, "Disclosure::layout gate", DisclosureState::is_expanded);
        // Retain collapsed descendant boxes for stable ownership, but exclude those stale boxes
        // from this node's derived overflow/content extent until expansion makes them active again.
        ctx.set_child_overflow_propagation(expanded);
        if !expanded {
            return;
        }

        let indent = self.indent(ctx.style());
        let spacing = ctx.style().spacing;
        let child_rect = Recti::new(
            rect.x.saturating_add(indent),
            rect.y.saturating_add(header_height).saturating_add(spacing),
            rect.width.saturating_sub(indent),
            rect.height.saturating_sub(header_height).saturating_sub(spacing),
        );
        runtime_update_state(&self.state, "Disclosure::layout", |state| {
            layout_children(ctx, &mut state.children, child_rect);
        });
    }

    fn children_visible(&self) -> bool {
        runtime_read_state(&self.state, "Disclosure::children_visible", DisclosureState::is_expanded)
    }

    fn route_input(&mut self, ctx: &mut ContainerInputCtx<'_>, event: &UiInputEvent) -> ContainerInputResult {
        if let Some(pos) = super::event_position(event) {
            self.header_hovered = self.header_rect.contains(&pos);
        }
        ctx.route_widget_in_rect(event, self.header_rect, self.opt)
    }
}

/// Builder associating [`DisclosureParameters`] with [`DisclosureContainer`].
pub struct DisclosureBuilder;

impl ContainerBuilder for DisclosureBuilder {
    type Parameters = DisclosureParameters;
    type W = DisclosureContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        DisclosureContainer {
            state: Rc::new(RefCell::new(DisclosureState {
                children: parameters.children,
                expanded: parameters.expanded,
            })),
            label: parameters.label,
            variant: parameters.variant,
            opt: parameters.opt,
            header_rect: Recti::default(),
            header_hovered: false,
        }
    }
}

/// Convenience constructor namespace for disclosure containers.
pub struct Disclosure;

impl Disclosure {
    /// Creates a state-owned disclosure and returns its weak state capability plus completed node.
    pub fn create(parameters: DisclosureParameters) -> (WidgetStateHandle<DisclosureState>, Node) {
        let container = DisclosureBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

fn layout_children(ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
    let count = children.len();
    let spacing = ctx.style().spacing;
    let available_height = rect.height.saturating_sub(spacing.saturating_mul(count.saturating_sub(1) as i32));
    let mut preferred = Vec::with_capacity(count);
    let mut policies = Vec::with_capacity(count);
    for index in 0..count {
        let child = children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(rect.width, available_height))
            .unwrap_or_default();
        preferred.push(child.height);
        policies.push(ctx.child_policy(children, index).map(|policy| policy.height).unwrap_or(crate::SizePolicy::Auto));
    }
    let heights = super::super::resolve_axis_tracks(&policies, &preferred, available_height);
    let mut y = rect.y;
    for index in 0..count {
        let height = heights.get(index).copied().unwrap_or_default();
        let _ = ctx.layout_child(children, index, Recti::new(rect.x, y, rect.width, height));
        y = y.saturating_add(height).saturating_add(spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_tree_parameters_preserve_the_retired_widget_capabilities() {
        let header = DisclosureBuilder::create_container(DisclosureParameters::header("Header", false, std::iter::empty()));
        assert_eq!(header.label, "Header");
        assert!(matches!(header.variant, DisclosureVariant::Header));
        assert!(header.widget_opt().intersects(WidgetOption::FRAME));
        let header_state = header.state_handle();
        assert_eq!(header_state.try_read(DisclosureState::is_collapsed), Some(true));
        header_state.try_update(DisclosureState::toggle).unwrap();
        assert_eq!(header_state.try_read(DisclosureState::is_expanded), Some(true));

        let custom_opt = WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT;
        let tree = DisclosureBuilder::create_container(DisclosureParameters::tree("Tree", true, std::iter::empty()).with_options(custom_opt));
        assert_eq!(tree.label, "Tree");
        assert!(matches!(tree.variant, DisclosureVariant::Tree));
        assert_eq!(tree.widget_opt().bits(), custom_opt.bits());
        assert_eq!(tree.state_handle().try_read(DisclosureState::is_expanded), Some(true));
    }
}
