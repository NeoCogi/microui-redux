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

use crate::render::Painter;
use crate::{AtlasHandle, ControlColor, Dimensioni, Recti, Style, WidgetEventPortHandle, WindowOption};

/// Active pointer gesture owned by manager-rendered window chrome.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum RootInteraction {
    /// No title or resize gesture is active.
    None,
    /// The title surface owns an in-progress move gesture.
    Moving,
    /// The bottom-right grip owns an in-progress resize gesture.
    Resizing,
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
    /// The user pressed the manager-owned close affordance.
    CloseRequested,
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
    /// Weak endpoint for geometry and close-request observations from this same window.
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
    /// Window close affordance.
    Close,
    /// Bottom-right resize grip.
    Resize,
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
    /// Application-content allocation after title and padding are removed.
    pub(super) body: Recti,
    /// Optional bottom-right resize allocation.
    pub(super) resize: Option<Recti>,
    /// Smallest valid outer size under the current chrome policy.
    pub(super) minimum_outer: Dimensioni,
    /// Intrinsic outer size produced from a measured application child.
    pub(super) intrinsic_outer: Dimensioni,
}

impl RootChromeGeometry {
    /// Classifies a screen-space point in interaction-priority order.
    pub(super) fn hit_test(self, point: crate::Vec2i) -> Option<RootChromePart> {
        // Specialized controls overlap title/body geometry and therefore take priority.
        if self.close.is_some_and(|rect| rect.contains(&point)) {
            Some(RootChromePart::Close)
        } else if self.resize.is_some_and(|rect| rect.contains(&point)) {
            Some(RootChromePart::Resize)
        } else if self.title.is_some_and(|rect| rect.contains(&point)) {
            Some(RootChromePart::Title)
        } else {
            None
        }
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
    let border = if options.intersects(WindowOption::FRAME) {
        style.frame_border().width.max(0)
    } else {
        0
    };
    // Both leading and trailing edges contribute to the outer extent.
    let border_extent = border.checked_mul(2).expect("root chrome frame extent overflowed i32");
    let padding_extent = padding.saturating_mul(2);
    let auto_width = options.intersects(WindowOption::AUTO_WIDTH);
    let auto_height = options.intersects(WindowOption::AUTO_HEIGHT);
    let mut minimum_width = if auto_width { 1 } else { 96 };
    let mut minimum_height = if auto_height { 1 } else { 64 };
    if !options.intersects(WindowOption::NO_TITLE) {
        // The title minimum retains enough room for text, padding, and the optional close button.
        let close_width = if options.intersects(WindowOption::NO_CLOSE) { 0 } else { title_height };
        let title_minimum_width = atlas
            .get_text_size(style.title_font, name)
            .width
            .saturating_add(close_width)
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
        minimum_width.checked_add(border_extent).expect("root chrome minimum width overflowed i32"),
        minimum_height.checked_add(border_extent).expect("root chrome minimum height overflowed i32"),
    );
    let title_extent = if options.intersects(WindowOption::NO_TITLE) { 0 } else { title_height };
    // Intrinsic geometry adds the surface-owned frame, title, and padding to child measurement.
    let intrinsic_outer = Dimensioni::new(
        child_intrinsic
            .width
            .saturating_add(padding_extent)
            .checked_add(border_extent)
            .expect("root chrome intrinsic width overflowed i32")
            .max(minimum_outer.width),
        child_intrinsic
            .height
            .saturating_add(padding_extent)
            .saturating_add(title_extent)
            .checked_add(border_extent)
            .expect("root chrome intrinsic height overflowed i32")
            .max(minimum_outer.height),
    );

    // Shared frame geometry supplies the client rectangle used by both paint and layout.
    let client = crate::ui_node::frame::frame_geometry(outer, options.intersects(WindowOption::FRAME), style).content_or_empty();
    let title =
        (!options.intersects(WindowOption::NO_TITLE)).then(|| Recti::new(client.x, client.y, client.width.max(0), title_height.min(client.height.max(0))));
    let close = title.and_then(|title| {
        (!options.intersects(WindowOption::NO_CLOSE)).then(|| {
            // A square close affordance occupies the trailing title edge.
            let width = title.height.min(title.width.max(0));
            let x = title.x.saturating_add(title.width).saturating_sub(width);
            Recti::new(x, title.y, width, title.height)
        })
    });
    let mut body = client;
    if let Some(title) = title {
        // Application content begins immediately below the title allocation.
        body.y = body.y.saturating_add(title.height);
        body.height = body.height.saturating_sub(title.height).max(0);
    }
    body = crate::expand_rect(body, -padding);
    body.width = body.width.max(0);
    body.height = body.height.max(0);
    let resize = (!options.intersects(WindowOption::AUTO_SIZE | WindowOption::NO_RESIZE)).then(|| {
        // The grip remains inside the client rectangle even for very small programmed roots.
        let size = style.scrollbar_size.max(0);
        let x = client.x.saturating_add(client.width).saturating_sub(size);
        let y = client.y.saturating_add(client.height).saturating_sub(size);
        Recti::new(x, y, size.min(client.width.max(0)), size.min(client.height.max(0)))
    });
    RootChromeGeometry {
        client,
        title,
        close,
        body,
        resize,
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

/// Records the frame or plain background that must appear behind application content.
pub(super) fn record_root_background(display_list: &mut crate::render::DisplayList, viewport: Recti, rect: Recti, options: WindowOption, style: &Style) {
    // Chrome uses a screen-space painter because it is outside the retained application tree.
    let mut painter = Painter::screen_space(display_list, viewport);
    if options.intersects(WindowOption::FRAME) {
        crate::ui_node::frame::paint_internal_frame(&mut painter, rect, Some(style.colors[ControlColor::WindowBG as usize]), style.frame_border());
    } else {
        painter.fill_rect(rect, style.colors[ControlColor::WindowBG as usize]);
    }
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
) {
    // Reuse committed geometry so hit-testing and painting cannot disagree within one UI commit.
    let mut painter = Painter::screen_space(display_list, viewport);
    if options.intersects(WindowOption::FRAME) {
        // Background recording already filled the framed interior before application content. Draw
        // only the border again in the overlay pass so an unclipped child may extend beyond the
        // parent body without covering parent-owned frame chrome.
        let mut border = style.frame_border();
        if active {
            // An active framed window keeps the same layout geometry while its inside-aligned
            // outline remains visible even when the ordinary theme border width is zero.
            border.width = border.width.max(1);
            border.color = style.window_focus_color;
        }
        painter.stroke_rect(outer, border.width, border.color);
    }
    if let Some(title) = geometry.title {
        let title_color = if active {
            style.window_focus_color
        } else {
            style.colors[ControlColor::TitleBG as usize]
        };
        painter.fill_rect(title, title_color);
        let mut text = title;
        if let Some(close) = geometry.close {
            // Reserve the trailing square so title text cannot paint beneath the close icon.
            text.width = close.x.saturating_sub(title.x).max(0);
        }
        if text.width > 0 && text.height > 0 {
            let color = style.colors[ControlColor::TitleText as usize];
            let position = crate::ui_node::text_layout::control_text_position_with_font(style, atlas, style.title_font, name, text, crate::WidgetOption::NONE);
            painter.with_clip(text, |painter| painter.text(style.title_font, name, position, color));
        }
        if let Some(close) = geometry.close {
            // The close glyph uses the same foreground role as the title text.
            painter.icon(style.icons.close, close, style.colors[ControlColor::TitleText as usize]);
        }
    }
    if let Some(visual) = geometry
        .resize
        .filter(|resize| resize.width > 0 && resize.height > 0)
        .and_then(|resize| resize.intersect(&geometry.client))
    {
        // The raised frame treatment keeps the grip visible over application content.
        crate::ui_node::frame::paint_internal_frame(&mut painter, visual, Some(style.colors[ControlColor::WindowBG as usize]), style.frame_border());
    }
}
