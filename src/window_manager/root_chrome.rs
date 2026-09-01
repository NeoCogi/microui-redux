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

//! Manager-owned window state, chrome geometry, and root lifecycle handles.
//!
//! Window chrome is not an application widget. [`super::WindowManager`] owns its state directly,
//! resolves chrome input before retained content routing, and records chrome around the application
//! display list. The retained [`crate::Node`] stored for a window therefore represents only the
//! application-authored content tree.

use std::fmt;

use crate::math::RectExt;
use crate::render::Painter;
use crate::{AppearanceRole, AtlasHandle, ControlColor, Dimensioni, Recti, Style, VisualState, WidgetEventPortHandle, WindowOption};

/// Active pointer gesture owned by manager-rendered window chrome.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootInteraction {
    /// No title or resize gesture is active.
    None,
    /// The title surface owns an in-progress move gesture.
    Moving,
    /// One resize edge or corner owns an in-progress geometry gesture.
    Resizing(RootResizeAxis),
    /// One caption button owns a press until the matching primary-button release.
    Caption(RootCaptionButton),
}

/// Window axes changed by one manager-owned resize region.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootResizeAxis {
    /// Right edge changes only window width.
    Width,
    /// Bottom edge changes only window height.
    Height,
    /// Bottom-right corner changes width and height together.
    Both,
}

/// Concrete manager-owned caption button selected by hit testing and capture.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootCaptionButton {
    /// Hides the retained window while preserving its state for explicit restoration.
    Minimize,
    /// Maximizes a normal window or restores a currently maximized window.
    Maximize,
    /// Hides the window and emits a close request.
    Close,
}

/// Semantic event emitted by manager-owned window chrome.
///
/// One concrete event stream replaces separate changed/submitted allocations while retaining an
/// explicit variant for each application-observable action.
#[derive(Copy, Clone, Debug)]
pub enum WindowEvent {
    /// A user move or resize committed a new authoritative outer rectangle.
    GeometryChanged {
        /// Complete outer rectangle after applying the interaction.
        rect: Recti,
    },
    /// The user released the manager-owned close affordance after a matching press.
    CloseRequested,
    /// The user minimized the window, making it locally invisible without destroying it.
    Minimized,
    /// The user maximized the window into its complete inherited viewport.
    Maximized {
        /// Complete maximized outer rectangle.
        rect: Recti,
    },
    /// The user restored the exact outer rectangle saved before maximization.
    Restored {
        /// Restored outer rectangle.
        rect: Recti,
    },
}

impl crate::WidgetEvent for WindowEvent {}

/// Cloneable non-owning capability for one manager-owned window or dialog.
///
/// Stable object identity and event delivery are deliberately separate fields. The private root ID
/// selects checked [`crate::Ui`] mutations and is never derived from an address; the weak typed
/// endpoint only subscribes to [`WindowEvent`] values. Cloning or dropping this
/// aggregate application handle never changes retained window ownership.
pub struct WindowHandle {
    /// Process-unique concrete identity used only by the owning window manager.
    id: super::RootId,
    /// Weak endpoint for geometry, caption, and visibility observations from this same window.
    events: WidgetEventPortHandle<WindowEvent>,
}

impl WindowHandle {
    /// Creates an application capability from an independently allocated identity and endpoint.
    pub(super) fn new(id: super::RootId, events: WidgetEventPortHandle<WindowEvent>) -> Self {
        // This private constructor is the sole pairing boundary. Manager registration creates both
        // values for the same root before either becomes observable to application code.
        Self { id, events }
    }

    /// Returns the private concrete key used by forest traversal and test diagnostics.
    pub(crate) const fn id(&self) -> super::RootId {
        // Copying the small typed value neither consults nor extends the event endpoint lifetime.
        self.id
    }

    /// Returns this window's weak typed event endpoint.
    pub fn events(&self) -> WidgetEventPortHandle<WindowEvent> {
        // Clone only the weak endpoint so subscription remains independent from stable identity and
        // cannot retain the manager-owned window.
        self.events.clone()
    }
}

impl Clone for WindowHandle {
    /// Clones the application capability without allocating a new identity or owning the window.
    fn clone(&self) -> Self {
        // Every clone must preserve the original ID/endpoint pairing established at registration.
        Self { id: self.id, events: self.events.clone() }
    }
}

impl fmt::Debug for WindowHandle {
    /// Reports endpoint liveness without exposing the private process-local identifier.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The numeric identity is intentionally absent because it is an implementation key rather
        // than an application-visible or persistent window identifier.
        f.debug_struct("WindowHandle").field("events", &self.events).finish_non_exhaustive()
    }
}

/// Window-chrome region selected by a pointer press.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootChromePart {
    /// Movable title surface excluding the close button.
    Title,
    /// One optional caption button.
    Caption(RootCaptionButton),
    /// One resize edge or the bottom-right corner.
    Resize(RootResizeAxis),
}

/// Immutable manager interaction snapshot consumed while recording one root's chrome.
#[derive(Copy, Clone)]
pub(super) struct RootChromeVisualState {
    /// Topmost chrome part under the pointer, if this root owns hover.
    pub(super) hovered: Option<RootChromePart>,
    /// Active chrome capture committed by the latest pointer event.
    pub(super) interaction: RootInteraction,
    /// Whether the maximize button currently represents restoration.
    pub(super) maximized: bool,
}

impl RootChromeVisualState {
    /// Resolves one part's hover and pressed facts into a complete theme state.
    fn part_state(self, part: RootChromePart) -> VisualState {
        let hovered = self.hovered == Some(part);
        let captured = match (self.interaction, part) {
            (RootInteraction::Moving, RootChromePart::Title) => true,
            (RootInteraction::Resizing(active), RootChromePart::Resize(part)) => active == part,
            (RootInteraction::Caption(active), RootChromePart::Caption(part)) => active == part,
            (RootInteraction::None | RootInteraction::Moving | RootInteraction::Resizing(_) | RootInteraction::Caption(_), _) => false,
        };
        // As with widgets, a captured pointer outside its originating part is no longer visually
        // pressed even though release routing remains captured by the manager.
        // This helper resolves only interaction within active chrome. The recording boundary
        // replaces the result with Inactive when the complete window is deactivated.
        VisualState::from_interaction(true, true, hovered, false, captured && hovered)
    }

    /// Resolves the complete window frame from any hovered or active resize region.
    fn frame_state(self) -> VisualState {
        let hovered = matches!(self.hovered, Some(RootChromePart::Resize(_)));
        let pressed = matches!(self.interaction, RootInteraction::Resizing(_)) && hovered;
        // Frame edges cannot own keyboard focus, so only hover and the matching capture contribute.
        VisualState::from_interaction(true, true, hovered, false, pressed)
    }
}

/// Complete derived geometry for one manager-owned root surface.
#[derive(Copy, Clone, Debug, Default)]
pub(super) struct RootChromeGeometry {
    /// Framed or unframed client rectangle inside the outer root rectangle.
    pub(super) client: Recti,
    /// Optional title allocation in screen coordinates.
    pub(super) title: Option<Recti>,
    /// Optional close-button allocation overlaying the title.
    pub(super) close: Option<Recti>,
    /// Optional minimize-button allocation overlaying the title.
    pub(super) minimize: Option<Recti>,
    /// Optional maximize-or-restore-button allocation overlaying the title.
    pub(super) maximize: Option<Recti>,
    /// Application-content allocation after title and padding are removed.
    pub(super) body: Recti,
    /// Optional right-edge width-resize allocation.
    pub(super) resize_right: Option<Recti>,
    /// Optional bottom-edge height-resize allocation.
    pub(super) resize_bottom: Option<Recti>,
    /// Optional bottom-right two-axis resize allocation and visible grip.
    pub(super) resize_corner: Option<Recti>,
    /// Smallest valid outer size under the current chrome policy.
    pub(super) minimum_outer: Dimensioni,
    /// Intrinsic outer size produced from a measured application child.
    pub(super) intrinsic_outer: Dimensioni,
}

/// Focused chrome geometry exposed only to crate-internal window behavior tests.
#[cfg(test)]
#[derive(Copy, Clone, Debug)]
pub(crate) struct DebugRootChromeControls {
    /// Optional minimize-button rectangle.
    pub(crate) minimize: Option<Recti>,
    /// Optional maximize-or-restore-button rectangle.
    pub(crate) maximize: Option<Recti>,
    /// Optional right-edge width-resize rectangle.
    pub(crate) resize_right: Option<Recti>,
    /// Optional bottom-edge height-resize rectangle.
    pub(crate) resize_bottom: Option<Recti>,
    /// Optional bottom-right two-axis resize rectangle.
    pub(crate) resize_corner: Option<Recti>,
}

impl RootChromeGeometry {
    /// Classifies a screen-space point in interaction-priority order.
    pub(super) fn hit_test(self, point: crate::Vec2i) -> Option<RootChromePart> {
        // Specialized controls overlap title/body geometry and therefore take priority.
        if self.close.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Caption(RootCaptionButton::Close))
        } else if self.maximize.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Caption(RootCaptionButton::Maximize))
        } else if self.minimize.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Caption(RootCaptionButton::Minimize))
        } else if self.resize_corner.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Resize(RootResizeAxis::Both))
        } else if self.resize_right.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Resize(RootResizeAxis::Width))
        } else if self.resize_bottom.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Resize(RootResizeAxis::Height))
        } else if self.title.is_some_and(|rect| rect.contains_point(point)) {
            Some(RootChromePart::Title)
        } else {
            None
        }
    }

    /// Returns one caption rectangle without exposing field-selection branches to input or paint.
    pub(super) const fn caption(self, button: RootCaptionButton) -> Option<Recti> {
        // The enum-to-field mapping is exhaustive, so adding a caption kind cannot silently skip
        // geometry in either the manager interaction or rendering path.
        match button {
            RootCaptionButton::Minimize => self.minimize,
            RootCaptionButton::Maximize => self.maximize,
            RootCaptionButton::Close => self.close,
        }
    }

    /// Returns one resize rectangle through the same typed axis used by pointer capture.
    pub(super) const fn resize(self, axis: RootResizeAxis) -> Option<Recti> {
        // Width, height, and combined interactions each retain one explicit committed hit region.
        match axis {
            RootResizeAxis::Width => self.resize_right,
            RootResizeAxis::Height => self.resize_bottom,
            RootResizeAxis::Both => self.resize_corner,
        }
    }

    /// Removes every resize region while retaining frame, caption, and body geometry.
    pub(super) fn without_resize(mut self) -> Self {
        // Maximized roots stay otherwise identical but cannot start a resize until restored.
        self.resize_right = None;
        self.resize_bottom = None;
        self.resize_corner = None;
        self
    }
}

/// Derives chrome, body, minimum, and intrinsic geometry from one outer rectangle.
pub(super) fn root_chrome_geometry(
    outer: Recti,
    child_intrinsic: Dimensioni,
    name: &str,
    options: WindowOption,
    style: &Style,
    atlas: &AtlasHandle,
) -> RootChromeGeometry {
    // Root padding belongs to the window surface rather than the application layout tree.
    let padding = if options.intersects(WindowOption::NO_PADDING) {
        0
    } else {
        style.padding.max(0)
    };
    let title_height = root_titlebar_height(style, atlas);
    let frame = if options.intersects(WindowOption::FRAME) {
        style.appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets.normalized()
    } else {
        crate::SliceInsets::ZERO
    };
    // Each pair of frame cells contributes independently to its outer axis.
    let horizontal_frame_extent = frame.horizontal_extent();
    let vertical_frame_extent = frame.vertical_extent();
    let padding_extent = padding.saturating_mul(2);
    let auto_width = options.intersects(WindowOption::AUTO_WIDTH);
    let auto_height = options.intersects(WindowOption::AUTO_HEIGHT);
    let mut minimum_width = if auto_width { 1 } else { 96 };
    let mut minimum_height = if auto_height { 1 } else { 64 };
    if !options.intersects(WindowOption::NO_TITLE) {
        // The title minimum retains enough room for text, padding, and every enabled caption button.
        let caption_count = i32::from(!options.intersects(WindowOption::NO_CLOSE))
            .saturating_add(i32::from(options.intersects(WindowOption::MINIMIZE_BUTTON)))
            .saturating_add(i32::from(options.intersects(WindowOption::MAXIMIZE_BUTTON)));
        let caption_width = title_height.saturating_mul(caption_count);
        let title_minimum_width = atlas
            .get_text_size(style.title_font, name)
            .width
            .saturating_add(caption_width)
            .saturating_add(padding_extent);
        minimum_width = minimum_width.max(title_minimum_width);
        minimum_height = minimum_height.max(if auto_height {
            title_height
        } else {
            title_height.saturating_add(padding_extent)
        });
    }
    // The frame contributes one border on every outer edge.
    let minimum_outer = Dimensioni::new(
        minimum_width.saturating_add(horizontal_frame_extent),
        minimum_height.saturating_add(vertical_frame_extent),
    );
    let title_extent = if options.intersects(WindowOption::NO_TITLE) { 0 } else { title_height };
    // Intrinsic geometry adds the surface-owned frame, title, and padding to child measurement.
    let intrinsic_outer = Dimensioni::new(
        child_intrinsic
            .width
            .saturating_add(padding_extent)
            .saturating_add(horizontal_frame_extent)
            .max(minimum_outer.width),
        child_intrinsic
            .height
            .saturating_add(padding_extent)
            .saturating_add(title_extent)
            .saturating_add(vertical_frame_extent)
            .max(minimum_outer.height),
    );

    // Shared frame geometry supplies the client rectangle used by both paint and layout.
    let frame_role = options.intersects(WindowOption::FRAME).then_some(crate::AppearanceRole::WindowFrame);
    let client = crate::ui_node::frame::frame_geometry(outer, frame_role, style).content_or_empty();
    let title =
        (!options.intersects(WindowOption::NO_TITLE)).then(|| Recti::new(client.x, client.y, client.width.max(0), title_height.min(client.height.max(0))));
    let (close, maximize, minimize) = if let Some(title) = title {
        // Allocate caption buttons from the trailing edge in conventional close/maximize/minimize
        // order. Each button is square unless a programmed tiny title exhausts remaining width.
        let mut trailing_x = title.x.saturating_add(title.width);
        let mut allocate = || {
            let remaining = trailing_x.saturating_sub(title.x).max(0);
            let width = title.height.min(remaining);
            trailing_x = trailing_x.saturating_sub(width);
            Recti::new(trailing_x, title.y, width, title.height)
        };
        let close = (!options.intersects(WindowOption::NO_CLOSE)).then(&mut allocate);
        let maximize = options.intersects(WindowOption::MAXIMIZE_BUTTON).then(&mut allocate);
        let minimize = options.intersects(WindowOption::MINIMIZE_BUTTON).then(&mut allocate);
        (close, maximize, minimize)
    } else {
        (None, None, None)
    };
    let mut body = client;
    if let Some(title) = title {
        // Application content begins immediately below the title allocation.
        body.y = body.y.saturating_add(title.height);
        body.height = body.height.saturating_sub(title.height).max(0);
    }
    body = crate::expand_rect(body, -padding);
    body.width = body.width.max(0);
    body.height = body.height.max(0);
    let (resize_right, resize_bottom, resize_corner) = if !options.intersects(WindowOption::AUTO_SIZE | WindowOption::NO_RESIZE) {
        // Edge hit thickness follows the configured window border and remains one pixel for an
        // unframed or zero-width flat style. The existing bottom-right grip keeps its larger,
        // style-controlled square while sharing the same exact outer far edges.
        let right_thickness = frame.right.max(1).min(outer.width.max(0));
        let bottom_thickness = frame.bottom.max(1).min(outer.height.max(0));
        let corner_size = style
            .scrollbar_size
            .max(right_thickness)
            .max(bottom_thickness)
            .max(1)
            .min(outer.width.max(0))
            .min(outer.height.max(0));
        let far_x = outer.x.saturating_add(outer.width);
        let far_y = outer.y.saturating_add(outer.height);
        let corner = Recti::new(far_x.saturating_sub(corner_size), far_y.saturating_sub(corner_size), corner_size, corner_size);
        let right = Recti::new(
            far_x.saturating_sub(right_thickness),
            outer.y,
            right_thickness,
            outer.height.saturating_sub(corner_size).max(0),
        );
        let bottom = Recti::new(
            outer.x,
            far_y.saturating_sub(bottom_thickness),
            outer.width.saturating_sub(corner_size).max(0),
            bottom_thickness,
        );
        (Some(right), Some(bottom), Some(corner))
    } else {
        (None, None, None)
    };
    RootChromeGeometry {
        client,
        title,
        close,
        minimize,
        maximize,
        body,
        resize_right,
        resize_bottom,
        resize_corner,
        minimum_outer,
        intrinsic_outer,
    }
}

/// Returns a title height large enough for the configured title font and padding.
fn root_titlebar_height(style: &Style, atlas: &AtlasHandle) -> i32 {
    // The style value acts as a minimum rather than allowing text to escape a too-short title.
    let font_height = atlas.get_font_height(style.title_font) as i32;
    let vertical_padding = (style.padding.max(0) / 2).max(1);
    let text_height = font_height.saturating_add(vertical_padding.saturating_mul(2));
    style.title_height.max(text_height)
}

/// Resolves frame artwork while preserving the inactive frame's structural border thickness.
fn root_frame_patch(style: &Style, active: bool, state: VisualState) -> crate::NinePatch {
    // WindowFrame is the single layout authority consumed by client geometry and resize hit tests.
    // Active artwork may replace every visual cell, but forcing its destination insets to this
    // authority prevents focus changes from moving application content or resize boundaries.
    let structural_insets = style.appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets;
    let role = if active {
        AppearanceRole::WindowFrameActive
    } else {
        AppearanceRole::WindowFrame
    };
    style.appearance(role, state).with_insets(structural_insets)
}

/// Records the frame or plain background that must appear behind application content.
pub(super) fn record_root_background(
    display_list: &mut crate::render::DisplayList,
    viewport: Recti,
    rect: Recti,
    style: &Style,
    active_frame: bool,
    deactivated: bool,
) {
    // Chrome uses a screen-space painter because it is outside the retained application tree.
    let mut painter = Painter::screen_space(display_list, viewport);
    // Record only the stretchable center below application content. Framed roots repeat their
    // eight edge cells in the overlay pass, avoiding duplicate border work while still protecting
    // chrome from overflowing descendants. Unframed roots use this same center-only body path.
    // Pointer interaction belongs to the frame edge and title controls, not the application body.
    // Resolve the body from Normal so merely crossing a resize edge cannot recolor the complete
    // window interior. Deactivation is an independent state rather than an inference from the
    // frame role: popup shells intentionally keep the passive frame role while remaining active.
    let state = if deactivated { VisualState::Inactive } else { VisualState::Normal };
    let patch = root_frame_patch(style, active_frame, state).with_insets(crate::SliceInsets::ZERO);
    let _ = crate::ui_node::frame::paint_internal_frame(&mut painter, rect, patch);
}

/// Records the frame border, title, and resize visuals that must appear above child content.
pub(super) fn record_root_overlay(
    display_list: &mut crate::render::DisplayList,
    viewport: Recti,
    outer: Recti,
    options: WindowOption,
    name: &str,
    geometry: RootChromeGeometry,
    style: &Style,
    atlas: &AtlasHandle,
    active: bool,
    visual: RootChromeVisualState,
) {
    // Reuse committed geometry so hit-testing and painting cannot disagree within one UI commit.
    let mut painter = Painter::screen_space(display_list, viewport);
    // Interaction affects active chrome only. Every chrome part in a deactivated root resolves the
    // dedicated state even if it still retains a hover or capture snapshot from the previous frame.
    let chrome_state = |state| if active { state } else { VisualState::Inactive };
    if options.intersects(WindowOption::FRAME) {
        // Background recording already filled the framed interior before application content. Draw
        // only the border again in the overlay pass so an unclipped child may extend beyond the
        // parent body without covering parent-owned frame chrome.
        painter.nine_patch(outer, root_frame_patch(style, active, chrome_state(visual.frame_state())).without_center());
    }
    if let Some(title) = geometry.title {
        let role = if active {
            AppearanceRole::WindowTitleActive
        } else {
            AppearanceRole::WindowTitle
        };
        let _ = crate::ui_node::frame::paint_internal_frame(
            &mut painter,
            title,
            style.appearance(role, chrome_state(visual.part_state(RootChromePart::Title))),
        );
        let mut text = title;
        let caption_start = [geometry.minimize, geometry.maximize, geometry.close]
            .into_iter()
            .flatten()
            .map(|caption| caption.x)
            .min();
        if let Some(caption_start) = caption_start {
            // Reserve every trailing caption square so title text cannot paint beneath any button.
            text.width = caption_start.saturating_sub(title.x).max(0);
        }
        if text.width > 0 && text.height > 0 {
            let color = if active {
                style.colors[ControlColor::TitleText as usize]
            } else {
                style.inactive_title_text_color
            };
            let position = crate::ui_node::text_layout::control_text_position_with_font(style, atlas, style.title_font, name, text, crate::WidgetOption::NONE);
            painter.with_clip(text, |painter| painter.text(style.title_font, name, position, color));
        }
        for button in [RootCaptionButton::Minimize, RootCaptionButton::Maximize, RootCaptionButton::Close] {
            if let Some(rect) = geometry.caption(button) {
                paint_caption_button(&mut painter, rect, button, style, active, visual);
            }
        }
    }
    if let Some(grip) = geometry
        .resize(RootResizeAxis::Both)
        .filter(|resize| resize.width > 0 && resize.height > 0)
        .and_then(|resize| resize.positive_intersection(geometry.client))
    {
        // The grip has an independent role so classic themes can supply dedicated corner artwork.
        let state = chrome_state(visual.part_state(RootChromePart::Resize(RootResizeAxis::Both)));
        let _ = crate::ui_node::frame::paint_internal_frame(&mut painter, grip, style.appearance(AppearanceRole::WindowResizeGrip, state));
    }
}

/// Records one stateful caption patch and its manager-owned flat fallback symbol.
fn paint_caption_button(painter: &mut Painter<'_>, rect: Recti, button: RootCaptionButton, style: &Style, window_active: bool, visual: RootChromeVisualState) {
    let role = match button {
        RootCaptionButton::Minimize => AppearanceRole::WindowMinimizeButton,
        RootCaptionButton::Maximize if visual.maximized => AppearanceRole::WindowRestoreButton,
        RootCaptionButton::Maximize => AppearanceRole::WindowMaximizeButton,
        RootCaptionButton::Close => AppearanceRole::WindowCloseButton,
    };
    let state = if window_active {
        visual.part_state(RootChromePart::Caption(button))
    } else {
        VisualState::Inactive
    };
    let Some(content) = crate::ui_node::frame::paint_internal_frame(painter, rect, style.appearance(role, state)) else {
        return;
    };
    let color = if window_active {
        style.colors[ControlColor::TitleText as usize]
    } else {
        style.inactive_title_text_color
    };
    match button {
        RootCaptionButton::Close => {
            // Close retains the atlas icon already required by every Style and test atlas.
            painter.icon(style.icons.close, content, color);
        }
        RootCaptionButton::Minimize => {
            // A centered lower horizontal stroke supplies a deterministic flat fallback over either
            // flat or image-backed button art without adding required atlas roles.
            let width = (content.width / 2).max(1);
            let x = content.x.saturating_add(content.width.saturating_sub(width) / 2);
            let y = content.y.saturating_add((content.height.saturating_mul(2) / 3).max(0));
            painter.fill_rect(Recti::new(x, y, width, 1.min(content.height.max(0))), color);
        }
        RootCaptionButton::Maximize if visual.maximized => {
            // Two offset outlines communicate restoration while staying legible at classic bitmap
            // title sizes. Clipping in Painter handles tiny caption allocations safely.
            let size = (content.width.min(content.height) / 2).max(1);
            let back = Recti::new(content.x.saturating_add(size / 3), content.y, size, size);
            let front = Recti::new(content.x, content.y.saturating_add(size / 3), size, size);
            painter.stroke_rect(back, 1, color);
            painter.stroke_rect(front, 1, color);
        }
        RootCaptionButton::Maximize => {
            // One centered outline is the flat maximize glyph; themed PNGs remain the background.
            let size = (content.width.min(content.height) / 2).max(1);
            let x = content.x.saturating_add(content.width.saturating_sub(size) / 2);
            let y = content.y.saturating_add(content.height.saturating_sub(size) / 2);
            painter.stroke_rect(Recti::new(x, y, size, size), 1, color);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Focused interaction-state tests for manager-owned chrome roles.

    use super::*;

    /// Verifies caption and resize capture select pressed art only over the originating region.
    #[test]
    fn chrome_visual_state_requires_matching_hover_for_pressed_art() {
        let close = RootChromePart::Caption(RootCaptionButton::Close);
        let right = RootChromePart::Resize(RootResizeAxis::Width);

        let close_pressed = RootChromeVisualState {
            hovered: Some(close),
            interaction: RootInteraction::Caption(RootCaptionButton::Close),
            maximized: false,
        };
        assert_eq!(close_pressed.part_state(close), VisualState::Pressed);

        let close_outside = RootChromeVisualState {
            hovered: Some(RootChromePart::Title),
            interaction: RootInteraction::Caption(RootCaptionButton::Close),
            maximized: false,
        };
        assert_eq!(close_outside.part_state(close), VisualState::Normal);

        let width_pressed = RootChromeVisualState {
            hovered: Some(right),
            interaction: RootInteraction::Resizing(RootResizeAxis::Width),
            maximized: false,
        };
        assert_eq!(width_pressed.part_state(right), VisualState::Pressed);
        assert_eq!(width_pressed.frame_state(), VisualState::Pressed);
    }
}
