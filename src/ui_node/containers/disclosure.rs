use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, ChildParticipation, Container, ControlColor, Dimensioni, Layout, MouseButton, Recti, Style, UiInputEvent, Widget, WidgetOption,
    WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetUpdateCtx,
};

use super::{Children, Column, ColumnParameters, ColumnState, ContainerLayoutCtx, Node};

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
    /// Weak access to content topology strongly owned by the body Column child.
    content: WidgetStateHandle<ColumnState>,
    expanded: bool,
}

impl WidgetState for DisclosureState {}

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
    pub fn len(&self) -> Option<usize> {
        self.content.try_read(ColumnState::len).flatten()
    }

    /// Returns whether the disclosure owns no children.
    pub fn is_empty(&self) -> Option<bool> {
        self.content.try_read(ColumnState::is_empty).flatten()
    }

    /// Appends one still-unmounted child.
    pub fn push(&mut self, node: Node) -> Result<(), Node> {
        self.content.try_update_with(node, |content, node| content.push(node))?
    }

    /// Inserts a node, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.content.try_update_with(node, |content, node| content.insert(index, node))?
    }

    /// Drops one indexed child owner and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.content.try_update(|content| content.remove_drop(index)).flatten()
    }

    /// Drops every child owner.
    pub fn clear(&mut self) -> Option<()> {
        self.content.try_update(ColumnState::clear).flatten()
    }

    /// Replaces all descendants in iterator order.
    pub fn replace<I>(&mut self, nodes: I) -> Result<(), I>
    where
        I: IntoIterator<Item = Node>,
    {
        self.content.try_update_with(nodes, |content, nodes| content.replace(nodes))?
    }
}

/// Dispatcher-addressable disclosure header.
///
/// The header owns no descendants and keeps only a weak reference to the state strongly owned by
/// [`DisclosureLayout`]. It therefore cannot extend the composite container's lifetime.
struct DisclosureHeader {
    state: Weak<RefCell<DisclosureState>>,
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
        let icon = atlas.get_icon_size(style.icons.expand);
        let text_width = if self.label.is_empty() {
            0
        } else {
            atlas.get_text_size(style.font, &self.label).width
        };
        let content_height = (font_height.max(icon.height) + vertical_pad * 2).max(0);
        let icon_width = (content_height - padding).max(icon.width);
        let content = Dimensioni::new((padding * 2 + icon_width + text_width).max(0), content_height);
        // A header frame is internal to this child, so preferred size must include its inset here.
        let border = if self.opt.intersects(WidgetOption::FRAME) {
            style.frame_border().width
        } else {
            0
        };
        crate::ui_node::frame::outer_preferred(content, border)
    }
}

impl Widget for DisclosureHeader {
    fn widget_opt(&self) -> &WidgetOption {
        // Static options describe only the header child, never the complete disclosure allocation.
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        // Header content is intrinsically one line and does not stretch to the offered bound.
        self.preferred(style, atlas)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Routing already selected this real child surface; only a left press submits a toggle.
        let submitted = matches!(input, Some(UiInputEvent::MouseDown { button, .. }) if button.intersects(MouseButton::LEFT));
        if !submitted {
            return;
        }
        // The weak link prevents this header from extending the enclosing composite's lifetime.
        let Some(state) = self.state.upgrade() else { return };
        runtime_update_state(&state, "DisclosureHeader::update", DisclosureState::toggle);
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Expansion determines the icon, while presentation variant determines background/frame.
        let Some(state) = self.state.upgrade() else { return };
        let expanded = runtime_read_state(&state, "DisclosureHeader::paint", DisclosureState::is_expanded);
        let mut row = ctx.local_rect();
        match self.variant {
            DisclosureVariant::Header => {
                let mut color = ControlColor::Button;
                if ctx.hovered() {
                    color.hover();
                }
                if self.opt.intersects(WidgetOption::FRAME) {
                    row = ctx.draw_internal_frame(row, color).unwrap_or_default();
                } else {
                    ctx.draw_rect(row, ctx.style().colors[color as usize]);
                }
            }
            DisclosureVariant::Tree if ctx.hovered() => {
                ctx.draw_rect(row, ctx.style().colors[ControlColor::ButtonHover as usize]);
            }
            DisclosureVariant::Tree => {}
        }

        // Reserve a square icon cell from row height, then paint text in the remaining rectangle.
        let text_color = ctx.style().colors[ControlColor::Text as usize];
        ctx.draw_icon(
            if expanded { ctx.style().icons.collapse } else { ctx.style().icons.expand },
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
        // Reuse ordinary option-derived press/release behavior for the addressable header child.
        crate::FocusPolicy::from_widget_options(self.opt)
    }
}

/// Geometry-only policy for the fixed body/header child roles.
pub struct DisclosureLayout {
    state: Rc<RefCell<DisclosureState>>,
    variant: DisclosureVariant,
}

impl DisclosureLayout {
    const BODY: usize = 0;
    const HEADER: usize = 1;

    /// Returns the tree-only body indentation in container-local coordinates.
    fn indent(&self, style: &Style) -> i32 {
        if matches!(self.variant, DisclosureVariant::Tree) {
            style.indent.max(0)
        } else {
            0
        }
    }
}

impl Layout for DisclosureLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        // The header always contributes. Body measurement is conditional so collapsed content does
        // not influence root auto-size while its state and nodes remain retained.
        let header = children.measure_child(Self::HEADER, style, atlas, available).unwrap_or_default();
        runtime_read_state(&self.state, "Disclosure::measure", |state| {
            if !state.expanded {
                return header;
            }
            let indent = self.indent(style);
            let spacing = style.spacing.max(0);
            let body_available = Dimensioni::new(
                if available.width > 0 {
                    available.width.saturating_sub(indent).max(1)
                } else {
                    0
                },
                if available.height > 0 {
                    available.height.saturating_sub(header.height).saturating_sub(spacing).max(1)
                } else {
                    0
                },
            );
            // BODY is itself a Column node, which measures the application-provided descendants.
            let body = children.measure_child(Self::BODY, style, atlas, body_available).unwrap_or_default();
            Dimensioni::new(
                header.width.max(body.width.saturating_add(indent)),
                header.height.saturating_add(spacing).saturating_add(body.height),
            )
        })
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Header is a fixed structural role and always occupies the first visible row.
        let preferred = children
            .measure_child(Self::HEADER, ctx.style(), ctx.atlas(), Dimensioni::new(rect.width, rect.height))
            .unwrap_or_default();
        let header_height = preferred.height.min(rect.height.max(0));
        let _ = ctx.set_child_participation(children, Self::HEADER, ChildParticipation::Active);
        let _ = ctx.layout_child(children, Self::HEADER, Recti::new(rect.x, rect.y, rect.width, header_height));

        // Participation is the single gate consumed by update, paint, hit testing, and target
        // sanitation. No generic visibility bit or topology rewrite is needed for collapse.
        let expanded = runtime_read_state(&self.state, "Disclosure::place", DisclosureState::is_expanded);
        let participation = if expanded { ChildParticipation::Active } else { ChildParticipation::Hidden };
        let _ = ctx.set_child_participation(children, Self::BODY, participation);
        if expanded {
            let indent = self.indent(ctx.style());
            let spacing = ctx.style().spacing.max(0);
            let body = Recti::new(
                rect.x.saturating_add(indent),
                rect.y.saturating_add(header_height).saturating_add(spacing),
                rect.width.saturating_sub(indent),
                rect.height.saturating_sub(header_height).saturating_sub(spacing),
            );
            let _ = ctx.layout_child(children, Self::BODY, body);
        }
    }
}

/// Builds the fixed body/header structure before wrapping it in the public owning node.
fn create_container(parameters: DisclosureParameters) -> (WidgetStateHandle<DisclosureState>, Container) {
    // A Column owns mutable application content. DisclosureState delegates topology operations to
    // its typed weak handle instead of duplicating another child collection.
    let (content, body) = Column::create(ColumnParameters::new(parameters.children.nodes));
    // This allocation is retained by DisclosureLayout and observed weakly by the header widget.
    let state = Rc::new(RefCell::new(DisclosureState { content, expanded: parameters.expanded }));
    let handle = WidgetStateHandle::new(&state);
    // The internal widget constructor avoids fabricating meaningless unit state for a surface whose
    // real state already belongs to the surrounding composite.
    let header = DisclosureHeader {
        state: Rc::downgrade(&state),
        label: parameters.label,
        variant: parameters.variant,
        opt: parameters.opt,
    };
    // Structural roles are stable: body at index zero, addressable header at index one.
    let layout = DisclosureLayout { state, variant: parameters.variant };
    let container = Container::new(layout, WidgetOption::NONE, [body, Node::widget_internal(header)]);
    (handle, container)
}

/// Convenience constructor namespace for disclosure containers.
pub struct Disclosure;

impl Disclosure {
    /// Creates a state-owned disclosure and returns its weak state capability plus completed node.
    ///
    /// Construction is complete before the node is returned: callers never observe or manipulate
    /// the two structural children separately.
    pub fn create(parameters: DisclosureParameters) -> (WidgetStateHandle<DisclosureState>, Node) {
        // Add the common Node runtime state around the concrete Container ownership boundary.
        let (state, container) = create_container(parameters);
        (state, Node::container(container))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_tree_preserve_state_and_fixed_structural_children() {
        let (header_state, header) = create_container(DisclosureParameters::header("Header", false, std::iter::empty()));
        assert_eq!(header_state.try_read(DisclosureState::is_collapsed), Some(true));
        header_state.try_update(DisclosureState::toggle).unwrap();
        assert_eq!(header_state.try_read(DisclosureState::is_expanded), Some(true));
        assert_eq!(Node::container(header).debug_node_count(), 3);

        let custom_opt = WidgetOption::ALIGN_RIGHT | WidgetOption::NO_INTERACT;
        let (tree_state, tree) = create_container(DisclosureParameters::tree("Tree", true, std::iter::empty()).with_options(custom_opt));
        assert_eq!(tree_state.try_read(DisclosureState::is_expanded), Some(true));
        assert_eq!(Node::container(tree).debug_node_count(), 3);
    }
}
