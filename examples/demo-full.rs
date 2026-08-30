//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Full retained-mode demo application.
//!
//! This example exercises core widgets, layout groups, scroll areas, images, and optional 3D
//! renderer integrations in one interactive application. A dedicated titleless layer-0 family root
//! fills the client area with its own menu and a perspective X-Y grid; its content-clipped child
//! windows float above the grid and below the root menu.
#[path = "./common/mod.rs"]
mod common;

use common::{
    application::Application,
    application::BackendInitContext,
    atlas_assets,
    camera::Camera,
    mesh::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex},
    obj_loader::Obj,
    polymesh::PolyMesh,
    view3d::View3D,
};
#[cfg(feature = "example-glow")]
use common::glow_renderer::GLRenderer as SelectedBackend;
#[cfg(all(not(feature = "example-glow"), feature = "example-vulkan"))]
use common::vulkan_renderer::VulkanRenderer as SelectedBackend;
#[cfg(all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"))]
use common::wgpu_renderer::WgpuRenderer as SelectedBackend;
use microui_redux::{prelude::*, render::Vertex};
use std::{cell::RefCell, f32::consts::PI, fs, path::PathBuf, rc::Rc, time::Instant};

type SelectedFrame<'a> = <SelectedBackend as RendererBackend>::Frame<'a>;

#[repr(C)]
pub struct TriVertex {
    pub pos: Vec2f,
    pub color: Color4b,
}

const TRI_VERTS: [TriVertex; 3] = [
    TriVertex {
        pos: Vec2f { x: 0.0, y: -1.0 },
        color: Color4b { x: 0xff, y: 0x00, z: 0x00, w: 0xff },
    },
    TriVertex {
        pos: Vec2f { x: -1.0, y: 1.0 },
        color: Color4b { x: 0x00, y: 0xff, z: 0x00, w: 0xff },
    },
    TriVertex {
        pos: Vec2f { x: 1.0, y: 1.0 },
        color: Color4b { x: 0x00, y: 0x00, z: 0xff, w: 0xff },
    },
];

struct TriangleState {
    angle: f32,
}

/// Half-width of the finite X-Y grid in world-space units.
const GRID_HALF_EXTENT: i32 = 20;
/// Interval between the visually stronger grid divisions.
const GRID_MAJOR_INTERVAL: i32 = 5;
/// Closest useful orbit radius, kept outside the grid's bounding sphere.
const GRID_MIN_CAMERA_DISTANCE: f32 = 32.0;
/// Furthest useful orbit radius before the finite grid becomes needlessly small.
const GRID_MAX_CAMERA_DISTANCE: f32 = 120.0;
/// Converts the example runner's integer wheel units into world-space camera movement.
const GRID_WHEEL_DISTANCE_SCALE: f32 = 0.1;

/// Application-owned view and presentation state shared by the grid widget and renderer.
struct Grid3dState {
    /// Arcball camera whose current PVM is consumed by the custom renderer.
    view_3d: View3D,
    /// Whether unit-spaced lines are drawn between the stronger five-unit divisions.
    show_minor_lines: bool,
}

/// Invisible retained widget that turns the custom-rendered grid into an input surface.
struct Grid3dWidget {
    /// Shared camera state; rendering observes the same value updated by pointer input.
    data: Rc<RefCell<Grid3dState>>,
    /// Claims drag focus and wheel delivery without contributing ordinary painted geometry.
    opt: WidgetOption,
}

/// Construction parameters for [`Grid3dWidget`].
struct Grid3dWidgetParameters {
    /// Application-owned grid state installed before the retained tree is committed.
    data: Rc<RefCell<Grid3dState>>,
}

impl WidgetParameters for Grid3dWidgetParameters {}

/// Typed builder used by `Node::custom_render` to pair input with backend rendering.
struct Grid3dWidgetBuilder;

impl WidgetBuilder for Grid3dWidgetBuilder {
    type Parameters = Grid3dWidgetParameters;
    type W = Grid3dWidget;

    /// Creates a scroll-grabbing leaf over the complete grid body.
    fn create_widget(parameters: Self::Parameters) -> Self::W {
        Self::W {
            data: parameters.data,
            opt: WidgetOption::GRAB_SCROLL,
        }
    }
}

impl Widget for Grid3dWidget {
    /// Exposes the interaction capabilities used by retained hit testing and routing.
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // The background grid accepts pointer focus for camera gestures but exposes no discrete
        // keyboard action, so sequential navigation skips this otherwise full-window surface.
        KeyboardBehavior::FOCUSABLE
    }

    /// Applies left-button arcball motion and wheel zoom to the shared grid camera.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let bounds = ctx.local_rect();
        if bounds.width <= 0 || bounds.height <= 0 {
            return;
        }

        // View3D normalizes pointer positions against its current dimensions, so synchronize the
        // retained allocation before interpreting this event. Coordinates are already local to
        // the widget and can therefore be passed directly to the arcball helper.
        let mut grid = self.data.borrow_mut();
        grid.view_3d.set_dimension(Dimensioni::new(bounds.width, bounds.height));

        match input {
            Some(UiInputEvent::MouseDrag { pos, delta, buttons }) if buttons.intersects(MouseButton::LEFT) => {
                let previous = *pos - *delta;
                let _ = grid.view_3d.update_drag(previous, *pos);
            }
            Some(UiInputEvent::Scroll { delta, .. }) => {
                // Prefer the conventional vertical wheel axis, while accepting horizontal wheel
                // devices as a useful fallback. Positive distance moves away from the origin.
                let wheel = if delta.y != 0 { delta.y } else { delta.x };
                if wheel != 0 {
                    let _ = grid
                        .view_3d
                        .apply_scroll_with_limits(wheel as f32 * GRID_WHEEL_DISTANCE_SCALE, GRID_MIN_CAMERA_DISTANCE, GRID_MAX_CAMERA_DISTANCE);
                }
            }
            _ => {}
        }
    }

    /// Paints nothing because the associated backend custom renderer owns all grid geometry.
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl LeafWidget for Grid3dWidget {
    /// Supplies a small intrinsic size; the fullscreen window's flex layout expands it to fit.
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(80, 24)
    }
}

struct PainterDemo {
    phase: f32,
    star_center: Option<Vec2f>,
    opt: WidgetOption,
}

struct PainterDemoParameters;

impl WidgetParameters for PainterDemoParameters {}

struct PainterDemoBuilder;

impl WidgetBuilder for PainterDemoBuilder {
    type Parameters = PainterDemoParameters;
    type W = PainterDemo;

    fn create_widget(_parameters: Self::Parameters) -> Self::W {
        PainterDemo {
            phase: 0.0,
            star_center: None,
            opt: WidgetOption::NONE,
        }
    }
}

impl Widget for PainterDemo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let bounds = ctx.local_rect();
        let local_width = bounds.width.max(0) as f32;
        let local_height = bounds.height.max(0) as f32;
        if local_width <= 0.0 || local_height <= 0.0 {
            return;
        }

        self.phase = (self.phase + 0.025) % (PI * 2.0);
        if ctx.hovered() {
            if let Some(
                UiInputEvent::MouseMove { pos, .. }
                | UiInputEvent::MouseDrag { pos, .. }
                | UiInputEvent::MouseDown { pos, .. }
                | UiInputEvent::MouseUp { pos, .. }
                | UiInputEvent::Scroll { pos, .. },
            ) = input
            {
                self.star_center = Some(Vec2f::new(pos.x as f32, pos.y as f32));
            }
        } else {
            self.star_center = None;
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        let local_width = bounds.width.max(0) as f32;
        let local_height = bounds.height.max(0) as f32;
        if local_width <= 0.0 || local_height <= 0.0 {
            return;
        }

        let clip_rect = rect(18, 18, (bounds.width - 36).max(0), (bounds.height - 36).max(0));
        let animated_center = Vec2f::new(
            local_width * 0.5 + self.phase.cos() * (local_width * 0.16),
            local_height * 0.5 + self.phase.sin() * (local_height * 0.12),
        );
        let star_center = if ctx.hovered() {
            self.star_center.unwrap_or(animated_center)
        } else {
            animated_center
        };
        let star_center = Vec2f::new(star_center.x.clamp(0.0, local_width), star_center.y.clamp(0.0, local_height));

        {
            let mut g = ctx.painter();
            let local = g.local_rect();
            let outer = rect(8, 8, (local.width - 16).max(0), (local.height - 16).max(0));
            let background = [
                Vec2f::new(0.0, 0.0),
                Vec2f::new(local_width, 0.0),
                Vec2f::new(local_width, local_height),
                Vec2f::new(0.0, local_height),
            ];

            g.fill_polygon(background.as_slice(), color(34, 38, 44, 255));
            for (start, end) in rect_edges(outer).into_iter().flatten() {
                g.stroke_line(start, end, 2.0, color(65, 70, 76, 255));
            }
            for (start, end) in rect_edges(clip_rect).into_iter().flatten() {
                g.stroke_line(start, end, 1.5, color(240, 210, 110, 255));
            }

            g.stroke_line(
                Vec2f::new(12.0, 12.0),
                Vec2f::new(local_width - 12.0, local_height - 12.0),
                3.0,
                color(70, 145, 220, 180),
            );
            g.stroke_line(
                Vec2f::new(local_width - 12.0, 12.0),
                Vec2f::new(12.0, local_height - 12.0),
                3.0,
                color(220, 95, 110, 180),
            );

            g.with_clip(clip_rect, |g| {
                for idx in 0..4 {
                    let t = self.phase + idx as f32 * 0.45;
                    let y = clip_rect.y as f32 + clip_rect.height as f32 * (0.15 + idx as f32 * 0.2);
                    g.stroke_line(
                        Vec2f::new(-32.0, y + t.sin() * 10.0),
                        Vec2f::new(local_width + 32.0, y + t.cos() * 26.0),
                        7.0 - idx as f32,
                        color(60 + idx as u8 * 30, 140 + idx as u8 * 18, 225, 130),
                    );
                }

                let star = build_star_polygon(star_center, 58.0, 26.0, 5, self.phase);
                g.fill_polygon(star.as_slice(), color(255, 180, 70, 225));

                let sweep = build_star_polygon(
                    Vec2f::new(local_width * 0.35 + self.phase.sin() * 18.0, local_height * 0.72),
                    34.0,
                    14.0,
                    4,
                    -self.phase * 1.3,
                );
                g.fill_polygon(sweep.as_slice(), color(90, 220, 180, 190));
            });
        }
    }
}

impl LeafWidget for PainterDemo {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(240, 200)
    }
}

const FALLOFF_MIN_NODE_GAP: f32 = 0.08;
const FALLOFF_HANDLE_X_MAX: f32 = 0.5;
const FALLOFF_PICK_RADIUS: f32 = 9.0;
const FALLOFF_SEGMENT_STEPS: usize = 24;

#[derive(Clone, Copy)]
struct FalloffNode {
    pos: Vec2f,
    in_x: f32,
    in_y: f32,
    out_x: f32,
    out_y: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FalloffTarget {
    Anchor(usize),
    InHandle(usize),
    OutHandle(usize),
}

struct FalloffEditor {
    nodes: Vec<FalloffNode>,
    active: Option<FalloffTarget>,
    hovered: Option<FalloffTarget>,
    opt: WidgetOption,
}

struct FalloffEditorParameters;

impl WidgetParameters for FalloffEditorParameters {}

struct FalloffEditorBuilder;

impl WidgetBuilder for FalloffEditorBuilder {
    type Parameters = FalloffEditorParameters;
    type W = FalloffEditor;

    fn create_widget(_parameters: Self::Parameters) -> Self::W {
        let mut editor = FalloffEditor {
            nodes: vec![
                FalloffNode {
                    pos: Vec2f::new(0.0, 1.0),
                    in_x: 0.0,
                    in_y: 1.0,
                    out_x: 0.22,
                    out_y: 1.0,
                },
                FalloffNode {
                    pos: Vec2f::new(0.23, 0.94),
                    in_x: 0.18,
                    in_y: 0.97,
                    out_x: 0.28,
                    out_y: 0.70,
                },
                FalloffNode {
                    pos: Vec2f::new(0.57, 0.31),
                    in_x: 0.24,
                    in_y: 0.46,
                    out_x: 0.24,
                    out_y: 0.10,
                },
                FalloffNode {
                    pos: Vec2f::new(1.0, 0.0),
                    in_x: 0.20,
                    in_y: 0.0,
                    out_x: 0.0,
                    out_y: 0.0,
                },
            ],
            active: None,
            hovered: None,
            opt: WidgetOption::NONE,
        };
        editor.sanitize();
        editor
    }
}

impl FalloffEditor {
    // The editor keeps a small inner margin so markers and thick strokes can sit inside the
    // widget without fighting the outer container frame.
    fn graph_rect(bounds: Recti) -> Recti {
        rect(14, 14, (bounds.width - 28).max(0), (bounds.height - 28).max(0))
    }

    // Endpoints stay pinned to a classic brush falloff shape while interior anchors remain sorted
    // on x. Handle x values are stored as relative factors in [0, 0.5], which guarantees every
    // segment satisfies P0.x <= P1.x <= P2.x <= P3.x and therefore stays x-monotone.
    fn sanitize(&mut self) {
        if self.nodes.len() < 2 {
            return;
        }

        let last = self.nodes.len() - 1;
        self.nodes[0].pos = Vec2f::new(0.0, 1.0);
        self.nodes[last].pos = Vec2f::new(1.0, 0.0);

        for idx in 0..self.nodes.len() {
            self.nodes[idx].in_x = self.nodes[idx].in_x.clamp(0.0, FALLOFF_HANDLE_X_MAX);
            self.nodes[idx].out_x = self.nodes[idx].out_x.clamp(0.0, FALLOFF_HANDLE_X_MAX);
            self.nodes[idx].in_y = self.nodes[idx].in_y.clamp(0.0, 1.0);
            self.nodes[idx].out_y = self.nodes[idx].out_y.clamp(0.0, 1.0);
            self.nodes[idx].pos.y = self.nodes[idx].pos.y.clamp(0.0, 1.0);
        }

        for idx in 1..last {
            let left = self.nodes[idx - 1].pos.x + FALLOFF_MIN_NODE_GAP;
            let right = self.nodes[idx + 1].pos.x - FALLOFF_MIN_NODE_GAP;
            self.nodes[idx].pos.x = if left <= right {
                self.nodes[idx].pos.x.clamp(left, right)
            } else {
                (left + right) * 0.5
            };
        }

        self.nodes[0].in_x = 0.0;
        self.nodes[0].in_y = self.nodes[0].pos.y;
        self.nodes[last].out_x = 0.0;
        self.nodes[last].out_y = self.nodes[last].pos.y;
    }

    // Converts normalized falloff coordinates into widget-local pixels. The editor stores data in
    // normalized space so the same control logic works no matter how the window is resized.
    fn graph_to_local(graph: Recti, point: Vec2f) -> Vec2f {
        let width = graph.width.max(1) as f32;
        let height = graph.height.max(1) as f32;
        Vec2f::new(
            graph.x as f32 + point.x.clamp(0.0, 1.0) * width,
            graph.y as f32 + (1.0 - point.y.clamp(0.0, 1.0)) * height,
        )
    }

    // Converts widget-local pixels back into normalized falloff coordinates and clamps them into
    // the visible graph domain so dragging outside the rect still yields stable endpoint behavior.
    fn local_to_graph(graph: Recti, point: Vec2f) -> Vec2f {
        let width = graph.width.max(1) as f32;
        let height = graph.height.max(1) as f32;
        Vec2f::new(
            ((point.x - graph.x as f32) / width).clamp(0.0, 1.0),
            (1.0 - (point.y - graph.y as f32) / height).clamp(0.0, 1.0),
        )
    }

    // Each incoming handle is parameterized relative to the span from the previous anchor. That
    // keeps the monotonicity invariant local to one segment and avoids cross-segment repair logic.
    fn in_handle_graph(&self, idx: usize) -> Vec2f {
        let node = self.nodes[idx];
        let prev = self.nodes[idx - 1].pos;
        let span = (node.pos.x - prev.x).max(0.0);
        Vec2f::new(node.pos.x - span * node.in_x, node.in_y)
    }

    // Outgoing handles use the same relative-x representation against the next anchor. Limiting
    // the factor to 0.5 guarantees the two handles for a segment cannot cross on x.
    fn out_handle_graph(&self, idx: usize) -> Vec2f {
        let node = self.nodes[idx];
        let next = self.nodes[idx + 1].pos;
        let span = (next.x - node.pos.x).max(0.0);
        Vec2f::new(node.pos.x + span * node.out_x, node.out_y)
    }

    fn target_local(&self, graph: Recti, target: FalloffTarget) -> Vec2f {
        match target {
            FalloffTarget::Anchor(idx) => Self::graph_to_local(graph, self.nodes[idx].pos),
            FalloffTarget::InHandle(idx) => Self::graph_to_local(graph, self.in_handle_graph(idx)),
            FalloffTarget::OutHandle(idx) => Self::graph_to_local(graph, self.out_handle_graph(idx)),
        }
    }

    // Exposes one cubic segment in Bernstein control-point form so sampling and drawing both reuse
    // the same handle reconstruction logic.
    fn segment_points(&self, seg: usize) -> [Vec2f; 4] {
        [
            self.nodes[seg].pos,
            self.out_handle_graph(seg),
            self.in_handle_graph(seg + 1),
            self.nodes[seg + 1].pos,
        ]
    }

    // Standard cubic Bezier evaluation. The editor relies on dense line sampling rather than
    // adding a dedicated curve primitive to the renderer.
    fn eval_segment(&self, seg: usize, t: f32) -> Vec2f {
        let [p0, p1, p2, p3] = self.segment_points(seg);
        let omt = 1.0 - t;
        let omt2 = omt * omt;
        let t2 = t * t;
        p0 * (omt2 * omt) + p1 * (3.0 * omt2 * t) + p2 * (3.0 * omt * t2) + p3 * (t2 * t)
    }

    // Samples the full piecewise curve in local pixels. A single sampled polyline feeds both the
    // filled-under-curve polygon and the visible stroke, which keeps draw work coherent.
    fn sample_curve_local(&self, graph: Recti, steps_per_segment: usize) -> Vec<Vec2f> {
        let steps = steps_per_segment.max(4);
        let mut points = Vec::with_capacity((self.nodes.len() - 1) * steps + 1);
        points.push(Self::graph_to_local(graph, self.nodes[0].pos));
        for seg in 0..self.nodes.len() - 1 {
            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                points.push(Self::graph_to_local(graph, self.eval_segment(seg, t)));
            }
        }
        points
    }

    // Hit testing is resolved in local pixel space because markers are displayed in pixels, not in
    // normalized graph units. Only draggable controls participate.
    fn pick_target(&self, graph: Recti, mouse_local: Vec2f) -> Option<FalloffTarget> {
        let mut best = None;
        let mut best_dist_sq = FALLOFF_PICK_RADIUS * FALLOFF_PICK_RADIUS;

        for idx in 0..self.nodes.len() {
            if idx > 0 && idx + 1 < self.nodes.len() {
                let target = FalloffTarget::Anchor(idx);
                let pos = self.target_local(graph, target);
                let dist_sq = (pos - mouse_local).length_squared();
                if dist_sq <= best_dist_sq {
                    best = Some(target);
                    best_dist_sq = dist_sq;
                }
            }

            if idx > 0 {
                let target = FalloffTarget::InHandle(idx);
                let pos = self.target_local(graph, target);
                let dist_sq = (pos - mouse_local).length_squared();
                if dist_sq <= best_dist_sq {
                    best = Some(target);
                    best_dist_sq = dist_sq;
                }
            }

            if idx + 1 < self.nodes.len() {
                let target = FalloffTarget::OutHandle(idx);
                let pos = self.target_local(graph, target);
                let dist_sq = (pos - mouse_local).length_squared();
                if dist_sq <= best_dist_sq {
                    best = Some(target);
                    best_dist_sq = dist_sq;
                }
            }
        }

        best
    }

    // Dragging writes directly back into the normalized representation. Anchor x is clamped
    // between neighboring anchors, while handle x updates only the relative factor that belongs to
    // its segment.
    fn drag_target(&mut self, target: FalloffTarget, point: Vec2f) {
        match target {
            FalloffTarget::Anchor(idx) => {
                if idx == 0 || idx + 1 == self.nodes.len() {
                    return;
                }

                let left = self.nodes[idx - 1].pos.x + FALLOFF_MIN_NODE_GAP;
                let right = self.nodes[idx + 1].pos.x - FALLOFF_MIN_NODE_GAP;
                self.nodes[idx].pos.x = if left <= right { point.x.clamp(left, right) } else { (left + right) * 0.5 };
                self.nodes[idx].pos.y = point.y.clamp(0.0, 1.0);
            }
            FalloffTarget::InHandle(idx) => {
                if idx == 0 {
                    return;
                }
                let anchor = self.nodes[idx].pos;
                let prev = self.nodes[idx - 1].pos;
                let span = (anchor.x - prev.x).max(FALLOFF_MIN_NODE_GAP * 0.25);
                self.nodes[idx].in_x = ((anchor.x - point.x) / span).clamp(0.0, FALLOFF_HANDLE_X_MAX);
                self.nodes[idx].in_y = point.y.clamp(0.0, 1.0);
            }
            FalloffTarget::OutHandle(idx) => {
                if idx + 1 >= self.nodes.len() {
                    return;
                }
                let anchor = self.nodes[idx].pos;
                let next = self.nodes[idx + 1].pos;
                let span = (next.x - anchor.x).max(FALLOFF_MIN_NODE_GAP * 0.25);
                self.nodes[idx].out_x = ((point.x - anchor.x) / span).clamp(0.0, FALLOFF_HANDLE_X_MAX);
                self.nodes[idx].out_y = point.y.clamp(0.0, 1.0);
            }
        }

        self.sanitize();
    }
}

impl Widget for FalloffEditor {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // Pointer focus keeps drag interaction stable without adding a non-functional Tab stop.
        KeyboardBehavior::FOCUSABLE
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let bounds = ctx.local_rect();
        let graph = Self::graph_rect(bounds);
        if graph.width <= 0 || graph.height <= 0 {
            return;
        }

        let mut changed = false;

        if !ctx.focused() && !ctx.active() {
            self.active = None;
        }

        let pointer = match input {
            Some(UiInputEvent::MouseMove { pos, delta } | UiInputEvent::MouseDrag { pos, delta, .. }) => Some((*pos, *delta)),
            Some(UiInputEvent::MouseDown { pos, .. } | UiInputEvent::MouseUp { pos, .. } | UiInputEvent::Scroll { pos, .. }) => Some((*pos, Vec2i::default())),
            _ => None,
        };
        let Some((mouse_pos, mouse_delta)) = pointer else { return };
        let mouse_local = Vec2f::new(mouse_pos.x as f32, mouse_pos.y as f32);
        self.hovered = if ctx.hovered() { self.pick_target(graph, mouse_local) } else { None };

        if ctx.clicked() {
            self.active = if graph.contains(&Vec2i::new(mouse_local.x as i32, mouse_local.y as i32)) {
                self.pick_target(graph, mouse_local)
            } else {
                None
            };
        } else if !ctx.active() {
            self.active = None;
        }

        if ctx.active()
            && (mouse_delta.x != 0 || mouse_delta.y != 0)
            && let Some(target) = self.active
        {
            let point = Self::local_to_graph(graph, mouse_local);
            self.drag_target(target, point);
            changed = true;
        }

        let _ = changed;
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        let bounds = ctx.local_rect();
        let graph = Self::graph_rect(bounds);
        if graph.width <= 0 || graph.height <= 0 {
            return;
        }

        let curve = self.sample_curve_local(graph, FALLOFF_SEGMENT_STEPS);

        {
            let mut g = ctx.painter();
            let local = g.local_rect();
            let background = [
                Vec2f::new(0.0, 0.0),
                Vec2f::new(local.width as f32, 0.0),
                Vec2f::new(local.width as f32, local.height as f32),
                Vec2f::new(0.0, local.height as f32),
            ];
            g.fill_polygon(background.as_slice(), color(25, 29, 34, 255));
            for (start, end) in rect_edges(rect(6, 6, (local.width - 12).max(0), (local.height - 12).max(0)))
                .into_iter()
                .flatten()
            {
                g.stroke_line(start, end, 1.5, color(62, 68, 76, 255));
            }
            for (start, end) in rect_edges(graph).into_iter().flatten() {
                g.stroke_line(start, end, 1.5, color(88, 96, 106, 255));
            }

            g.with_clip(graph, |g| {
                for idx in 1..4 {
                    let x = graph.x as f32 + graph.width as f32 * idx as f32 / 4.0;
                    let y = graph.y as f32 + graph.height as f32 * idx as f32 / 4.0;
                    g.stroke_line(
                        Vec2f::new(x, graph.y as f32),
                        Vec2f::new(x, (graph.y + graph.height) as f32),
                        1.0,
                        color(46, 53, 60, 255),
                    );
                    g.stroke_line(
                        Vec2f::new(graph.x as f32, y),
                        Vec2f::new((graph.x + graph.width) as f32, y),
                        1.0,
                        color(46, 53, 60, 255),
                    );
                }

                let mut fill = Vec::with_capacity(curve.len() + 2);
                fill.extend(curve.iter().copied());
                fill.push(Vec2f::new((graph.x + graph.width) as f32, (graph.y + graph.height) as f32));
                fill.push(Vec2f::new(graph.x as f32, (graph.y + graph.height) as f32));
                g.fill_polygon(fill.as_slice(), color(74, 156, 216, 70));

                for pair in curve.windows(2) {
                    g.stroke_line(pair[0], pair[1], 3.0, color(111, 205, 251, 255));
                }
            });

            for seg in 0..self.nodes.len() - 1 {
                let anchor_a = Self::graph_to_local(graph, self.nodes[seg].pos);
                let anchor_b = Self::graph_to_local(graph, self.nodes[seg + 1].pos);
                let handle_a = Self::graph_to_local(graph, self.out_handle_graph(seg));
                let handle_b = Self::graph_to_local(graph, self.in_handle_graph(seg + 1));
                g.stroke_line(anchor_a, handle_a, 1.5, color(154, 122, 88, 255));
                g.stroke_line(handle_b, anchor_b, 1.5, color(154, 122, 88, 255));
            }

            for idx in 0..self.nodes.len() {
                if idx > 0 {
                    let target = FalloffTarget::InHandle(idx);
                    let center = self.target_local(graph, target);
                    let radius = if self.active == Some(target) {
                        6.0
                    } else if self.hovered == Some(target) {
                        5.0
                    } else {
                        4.0
                    };
                    let marker = build_diamond_polygon(center, radius);
                    g.fill_polygon(marker.as_slice(), color(235, 194, 92, 255));
                }

                if idx + 1 < self.nodes.len() {
                    let target = FalloffTarget::OutHandle(idx);
                    let center = self.target_local(graph, target);
                    let radius = if self.active == Some(target) {
                        6.0
                    } else if self.hovered == Some(target) {
                        5.0
                    } else {
                        4.0
                    };
                    let marker = build_diamond_polygon(center, radius);
                    g.fill_polygon(marker.as_slice(), color(235, 194, 92, 255));
                }

                let center = Self::graph_to_local(graph, self.nodes[idx].pos);
                let target = FalloffTarget::Anchor(idx);
                let radius = if idx == 0 || idx + 1 == self.nodes.len() {
                    4.5
                } else if self.active == Some(target) {
                    6.5
                } else if self.hovered == Some(target) {
                    5.5
                } else {
                    5.0
                };
                let marker = build_square_polygon(center, radius);
                let color = if idx == 0 || idx + 1 == self.nodes.len() {
                    color(220, 228, 236, 255)
                } else {
                    color(250, 250, 250, 255)
                };
                g.fill_polygon(marker.as_slice(), color);
            }
        }
    }
}

impl LeafWidget for FalloffEditor {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(300, 220)
    }
}

struct SuzanneData {
    view_3d: View3D,
    mesh: MeshBuffers,
}

struct SuzanneWidget {
    data: Rc<RefCell<SuzanneData>>,
    opt: WidgetOption,
}

struct SuzanneWidgetParameters {
    data: Rc<RefCell<SuzanneData>>,
}

impl WidgetParameters for SuzanneWidgetParameters {}

struct SuzanneWidgetBuilder;

impl WidgetBuilder for SuzanneWidgetBuilder {
    type Parameters = SuzanneWidgetParameters;
    type W = SuzanneWidget;

    fn create_widget(parameters: Self::Parameters) -> Self::W {
        SuzanneWidget {
            data: parameters.data,
            opt: WidgetOption::GRAB_SCROLL,
        }
    }
}

impl Widget for SuzanneWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // The demo viewport intentionally remains a Tab stop because its update handler supports
        // arrow-key orbiting and text-input W/S zoom alongside pointer gestures.
        KeyboardBehavior::TAB_STOP
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        let bounds = ctx.local_rect();
        if bounds.width <= 0 || bounds.height <= 0 {
            return;
        }

        let mut suzanne = self.data.borrow_mut();

        suzanne.view_3d.set_dimension(Dimensioni::new(bounds.width, bounds.height));
        let handled_drag = if let Some(UiInputEvent::MouseDrag { pos, delta, buttons }) = input {
            if buttons.intersects(MouseButton::LEFT) {
                let prev = *pos - *delta;
                let _ = suzanne.view_3d.update_drag(prev, *pos);
                true
            } else {
                false
            }
        } else {
            false
        };

        if let Some(UiInputEvent::Scroll { delta, .. }) = input {
            let axis = if delta.y != 0 { delta.y } else { delta.x };
            if axis != 0 {
                suzanne.view_3d.apply_scroll(axis as f32);
            }
        }

        if !handled_drag && !matches!(input, Some(UiInputEvent::Scroll { .. })) {
            let step = 20;
            let mut delta = Vec2i::new(0, 0);
            let pressed_key = match input {
                Some(UiInputEvent::Key { event }) if event.is_pressed() => Some(event.key),
                _ => None,
            };
            match pressed_key {
                Some(Key::ArrowLeft) => delta.x -= step,
                Some(Key::ArrowRight) => delta.x += step,
                Some(Key::ArrowUp) => delta.y -= step,
                Some(Key::ArrowDown) => delta.y += step,
                _ => {}
            }
            if delta.x != 0 || delta.y != 0 {
                let center = Vec2i::new(bounds.width / 2, bounds.height / 2);
                let curr = center + delta;
                suzanne.view_3d.update_drag(center, curr);
            }
            let text = match input {
                Some(UiInputEvent::Text { text }) => text.as_str(),
                _ => "",
            };
            for ch in text.chars() {
                match ch {
                    'w' | 'W' => {
                        let _ = suzanne.view_3d.apply_scroll(-0.5);
                    }
                    's' | 'S' => {
                        let _ = suzanne.view_3d.apply_scroll(0.5);
                    }
                    _ => {}
                }
            }
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl LeafWidget for SuzanneWidget {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(80, 24)
    }
}

fn stateful_leaf<B: WidgetBuilder>(parameters: B::Parameters) -> (TypedWidgetHandle<B::W>, Node) {
    Node::typed_widget(B::create_widget(parameters))
}

trait IntoDemoNode {
    fn into_demo_node(self) -> Node;
}

impl IntoDemoNode for Node {
    fn into_demo_node(self) -> Node {
        self
    }
}

impl IntoDemoNode for Custom {
    fn into_demo_node(self) -> Node {
        Node::widget(self)
    }
}

impl IntoDemoNode for PainterDemo {
    fn into_demo_node(self) -> Node {
        Node::widget(self)
    }
}

impl IntoDemoNode for FalloffEditor {
    fn into_demo_node(self) -> Node {
        Node::widget(self)
    }
}

impl IntoDemoNode for SuzanneWidget {
    fn into_demo_node(self) -> Node {
        Node::widget(self)
    }
}

/// Small example-local authoring scope that immediately moves unique nodes into concrete
/// state-owned containers. It carries no identity or reconciliation metadata.
#[derive(Default)]
struct DemoNodes {
    items: Vec<LinearItem>,
}

impl DemoNodes {
    fn build(f: impl FnOnce(&mut Self)) -> Vec<LinearItem> {
        let mut nodes = Self::default();
        f(&mut nodes);
        nodes.items
    }

    fn children(f: impl FnOnce(&mut Self)) -> Vec<Node> {
        Self::build(f).into_iter().map(LinearItem::into_node).collect()
    }

    fn items(f: impl FnOnce(&mut Self)) -> Vec<LinearItem> {
        Self::build(f)
    }

    fn push(&mut self, node: Node) {
        self.items.push(LinearItem::content(node));
    }

    fn push_item(&mut self, item: LinearItem) {
        self.items.push(item);
    }

    fn with_track(&mut self, track: TrackSize) -> DemoNode<'_> {
        DemoNode { nodes: self, track, fixed_cross: None }
    }

    fn with_fixed_cross(&mut self, extent: i32) -> DemoNode<'_> {
        DemoNode {
            nodes: self,
            track: TrackSize::Content,
            fixed_cross: Some(extent),
        }
    }

    fn widget<W: IntoDemoNode>(&mut self, widget: W) {
        self.push(widget.into_demo_node());
    }

    fn text_with_wrap(&mut self, text: impl Into<String>, wrap: TextWrap) {
        let (_, node) = TextBlock::create(TextBlockParameters::with_wrap(text, wrap));
        self.push(node);
    }

    fn header(&mut self, label: impl Into<String>, expanded: bool, f: impl FnOnce(&mut Self)) -> TypedWidgetHandle<Disclosure> {
        let (state, node) = Disclosure::create(DisclosureParameters::header(label, expanded, Self::items(f)));
        self.push(node);
        state
    }

    fn tree_node(&mut self, label: impl Into<String>, expanded: bool, f: impl FnOnce(&mut Self)) -> TypedWidgetHandle<Disclosure> {
        let (state, node) = Disclosure::create(DisclosureParameters::tree(label, expanded, Self::items(f)));
        self.push(node);
        state
    }

    fn row(&mut self, widths: &[TrackSize], cross_size: LinearCrossSize, f: impl FnOnce(&mut Self)) {
        let items = Self::children(f)
            .into_iter()
            .enumerate()
            .map(|(index, node)| LinearItem::new(node, widths.get(index).copied().unwrap_or(TrackSize::Content)));
        let (_, node) = Linear::create(LinearParameters::horizontal(items).with_cross_size(cross_size));
        // The surrounding vertical Linear owns this horizontal Linear's main-axis slot. Translate
        // the shared cross behavior back to that parent-child relationship explicitly.
        let parent_track = match cross_size {
            LinearCrossSize::Content => TrackSize::Content,
            LinearCrossSize::Fixed(extent) => TrackSize::Fixed(extent),
            LinearCrossSize::Stretch => TrackSize::Flex(1.0),
        };
        self.push_item(LinearItem::new(node, parent_track));
    }

    /// Adds a Grid with an explicit height relationship to this surrounding vertical Linear.
    fn grid(&mut self, widths: &[TrackSize], heights: &[TrackSize], height: TrackSize, f: impl FnOnce(&mut Self)) {
        let items = Self::children(f).into_iter().map(GridItem::new);
        let (_, node) = Grid::create(GridParameters::new(widths.iter().copied(), heights.iter().copied(), items));
        self.push_item(LinearItem::new(node, height));
    }

    fn column(&mut self, f: impl FnOnce(&mut Self)) {
        let (_, node) = Linear::create(LinearParameters::vertical(Self::items(f)));
        self.push(node);
    }

    fn fixed_column(&mut self, height: i32, reversed: bool, f: impl FnOnce(&mut Self)) -> TypedWidgetHandle<Linear> {
        let items = Self::children(f).into_iter().map(|node| LinearItem::fixed(node, height));
        let parameters = if reversed {
            LinearParameters::vertical(items).reversed()
        } else {
            LinearParameters::vertical(items)
        };
        let (state, node) = Linear::create(parameters);
        self.push(node);
        state
    }
}

struct DemoNode<'a> {
    nodes: &'a mut DemoNodes,
    track: TrackSize,
    fixed_cross: Option<i32>,
}

impl DemoNode<'_> {
    fn widget<W: IntoDemoNode>(self, widget: W) {
        let mut item = LinearItem::new(widget.into_demo_node(), self.track);
        if let Some(extent) = self.fixed_cross {
            item = item.with_fixed_cross(extent);
        }
        self.nodes.push_item(item);
    }

    fn scroll_area(self, opt: ScrollAreaOption, f: impl FnOnce(&mut DemoNodes)) {
        let (_, content) = Linear::create(LinearParameters::vertical(DemoNodes::items(f)));
        let (_, node) = ScrollArea::create(ScrollAreaParameters::new(opt, content));
        let mut item = LinearItem::new(node, self.track);
        if let Some(extent) = self.fixed_cross {
            item = item.with_fixed_cross(extent);
        }
        self.nodes.push_item(item);
    }

    fn custom_render<B, W>(self, widget: W, renderer: CustomRenderHandle<B>)
    where
        B: RendererBackend,
        W: LeafWidget + 'static,
    {
        let mut item = LinearItem::new(Node::custom_render(widget, renderer), self.track);
        if let Some(extent) = self.fixed_cross {
            item = item.with_fixed_cross(extent);
        }
        self.nodes.push_item(item);
    }
}

struct DemoRootContents {
    style: TypedWidgetHandle<Linear>,
    log: TypedWidgetHandle<Linear>,
    typography: TypedWidgetHandle<Linear>,
    triangle: TypedWidgetHandle<Linear>,
    painter: TypedWidgetHandle<Linear>,
    falloff: TypedWidgetHandle<Linear>,
    suzanne: TypedWidgetHandle<Linear>,
    stack_direction: TypedWidgetHandle<Linear>,
    weight: TypedWidgetHandle<Linear>,
    demo: TypedWidgetHandle<Linear>,
    combo: TypedWidgetHandle<Linear>,
    popup: TypedWidgetHandle<Linear>,
}

fn root_content() -> (TypedWidgetHandle<Linear>, Node) {
    Linear::create(LinearParameters::vertical(std::iter::empty::<LinearItem>()))
}

fn replace_root_content(root: &TypedWidgetHandle<Linear>, items: Vec<LinearItem>, name: &str) {
    if !matches!(root.try_update_with(items, Linear::replace), Ok(Ok(()))) {
        panic!("{name} root content state unavailable");
    }
}

fn retained_leaf<B: WidgetBuilder>(parameters: B::Parameters) -> Node {
    stateful_leaf::<B>(parameters).1
}

fn static_label(text: impl Into<String>) -> Node {
    retained_leaf::<ListItemBuilder>(ListItemParameters::with_opt(text, WidgetOption::NO_INTERACT))
}

fn centered_button(label: impl Into<String>) -> (WidgetEventPortHandle<ButtonSubmitted>, Node) {
    let (button, node) = Button::create(ButtonParameters::with_opt(label, WidgetOption::FRAME | WidgetOption::ALIGN_CENTER));
    let submitted = button.submitted();
    (submitted, node)
}

/// Concrete menu item handles retained only where application state mutates live presentation.
struct DemoMenuItems {
    /// Open is disabled while the independent file dialog is active.
    open_file: MenuItemHandle,
    /// Auto-scroll reflects the current log-following state.
    auto_scroll: MenuItemHandle,
    /// Comfortable is one half of the spacing radio pair.
    comfortable_spacing: MenuItemHandle,
    /// Compact is the other half of the spacing radio pair.
    compact_spacing: MenuItemHandle,
}

/// Concrete item handles whose presentation mirrors the fullscreen grid state.
struct GridMenuItems {
    /// Checked item controlling whether the renderer includes unit-spaced grid lines.
    show_minor_lines: MenuItemHandle,
}

/// Creates and subscribes one concrete item before its value enters the menu hierarchy.
fn registered_menu_item(
    context: &mut Context<SelectedBackend, State>,
    parameters: MenuItemParameters,
    handler: for<'a> fn(&mut State, &mut Ui<'a>, &MenuItemSubmitted),
) -> (MenuItemHandle, MenuItem) {
    // The handle keeps stable presentation identity and projects a separate weak submission
    // endpoint, while the uniquely owned value enters exactly one declarative menu position.
    let (handle, item) = MenuItem::create(parameters);
    context
        .subscribe_context(handle.submitted(), handler)
        .expect("new menu item must be unsubscribed");
    (handle, item)
}

/// Builds the compact declarative menu hierarchy used by the main demo window.
fn demo_menu_bar(context: &mut Context<SelectedBackend, State>) -> (MenuBar, DemoMenuItems) {
    let (new_session, new_session_item) =
        registered_menu_item(context, MenuItemParameters::new("New Session").shortcut_hint("Ctrl+N"), State::menu_new_session);
    let (open_file, open_file_item) = registered_menu_item(context, MenuItemParameters::new("Open...").shortcut_hint("Ctrl+O"), State::menu_open_file);
    let (_, save_snapshot_item) = MenuItem::create(MenuItemParameters::new("Save Snapshot").shortcut_hint("Ctrl+S").disabled());
    let (clear_log, clear_log_item) = registered_menu_item(context, MenuItemParameters::new("Clear Log"), State::menu_clear_log);
    let (_, exit_item) = MenuItem::create(MenuItemParameters::new("Exit").disabled());

    let (auto_scroll, auto_scroll_item) = registered_menu_item(
        context,
        MenuItemParameters::new("Auto-scroll Log").checked(true),
        State::menu_toggle_auto_scroll,
    );
    let (comfortable_spacing, comfortable_item) = registered_menu_item(
        context,
        MenuItemParameters::new("Comfortable Spacing").radio(true),
        State::menu_comfortable_spacing,
    );
    let (compact_spacing, compact_item) = registered_menu_item(context, MenuItemParameters::new("Compact Spacing").radio(false), State::menu_compact_spacing);

    let (about, about_item) = registered_menu_item(context, MenuItemParameters::new("About microui-redux"), State::menu_about);

    // Items move directly into the compact declaration. No row nodes, cell nodes, or per-item
    // presentation widgets are retained solely to express this hierarchy.
    let menu_bar = MenuBar::new([
        Menu::new("File")
            .item(new_session_item)
            .item(open_file_item)
            .item(save_snapshot_item)
            .separator()
            .item(clear_log_item)
            .separator()
            .item(exit_item),
        Menu::new("View")
            .item(auto_scroll_item)
            .separator()
            .submenu(Menu::new("Log Spacing").item(comfortable_item).item(compact_item)),
        Menu::new("Help").item(about_item),
    ]);

    // Handles without later presentation changes are intentionally dropped. The moved `MenuItem`
    // values own their state, while Context retains the typed subscriptions used for dispatch.
    drop((new_session, clear_log, about));
    (
        menu_bar,
        DemoMenuItems {
            open_file,
            auto_scroll,
            comfortable_spacing,
            compact_spacing,
        },
    )
}

/// Builds the independent compact menu hierarchy owned by the fullscreen X-Y grid surface.
fn grid_menu_bar(context: &mut Context<SelectedBackend, State>) -> (MenuBar, GridMenuItems) {
    // Grid items use the same direct typed subscriptions as the floating window's items. Popup
    // ownership and closure follow from the MenuBar installed on the grid Window below.
    let (reset_view, reset_view_item) = registered_menu_item(context, MenuItemParameters::new("Reset View"), State::grid_reset_view);
    let (show_minor_lines, show_minor_lines_item) = registered_menu_item(
        context,
        MenuItemParameters::new("Minor Grid Lines").checked(true),
        State::grid_toggle_minor_lines,
    );
    let (about_grid, about_grid_item) = registered_menu_item(context, MenuItemParameters::new("About X-Y Grid"), State::grid_about);

    // This bar is intentionally separate from the floating demo's File/View/Help menus. Installing
    // it on the grid Window makes every generated popup belong to the layer-zero grid window.
    let menu_bar = MenuBar::new([
        Menu::new("Grid").item(reset_view_item).separator().item(show_minor_lines_item),
        Menu::new("Help").item(about_grid_item),
    ]);

    // These command handles need no later presentation updates; their moved item values stay alive
    // in the bar's compact menu data and continue to feed the already-subscribed event sources.
    drop((reset_view, about_grid));
    (menu_bar, GridMenuItems { show_minor_lines })
}

fn set_slider_value(state: &TypedWidgetHandle<Slider>, value: Real) {
    state.set_value(value).expect("slider unavailable");
}

struct DemoRuntimes {
    bg_sliders: [Node; 3],
    style_color_sliders: [Node; 64],
    style_value_sliders: [Node; 5],
    submit_buf: Node,
    text_area: Node,
    combo: Node,
    combo_items: [Node; 4],
    style_color_labels: [Node; 16],
    style_color_swatches: [Node; 16],
    style_metric_labels: [Node; 5],
    stack_direction_labels: [Node; 2],
    weight_labels: [Node; 2],
    window_info_labels: [Node; 3],
    window_info_values: [Node; 3],
    test_button_labels: [Node; 3],
    tree_labels: [Node; 2],
    background_labels: [Node; 3],
    submit_button: Node,
    log_scroll: Node,
    typography_heading: Node,
    typography_body: Node,
    typography_button: Node,
    test_buttons: [Node; 6],
    tree_buttons: [Node; 6],
    popup_buttons: [Node; 2],
    texture_buttons: [Node; 4],
    stack_direction_buttons: [Node; 6],
    weight_buttons: [Node; 9],
    external_image_button: Option<Node>,
    checkboxes: [Node; 3],
    triangle_renderer: CustomRenderHandle<SelectedBackend>,
    suzanne_renderer: CustomRenderHandle<SelectedBackend>,
    triangle_widget: Custom,
    painter_widget: PainterDemo,
    falloff_widget: FalloffEditor,
    suzanne_widget: SuzanneWidget,
    background_swatch: Node,
}

struct State {
    bg: [Real; 3],
    bg_slider_states: [TypedWidgetHandle<Slider>; 3],
    bg_slider_changed: [WidgetEventPortHandle<SliderChanged>; 3],
    style_color_slider_states: [TypedWidgetHandle<Slider>; 64],
    style_color_slider_changed: [WidgetEventPortHandle<SliderChanged>; 64],
    style_value_slider_states: [TypedWidgetHandle<Slider>; 5],
    style_value_slider_changed: [WidgetEventPortHandle<SliderChanged>; 5],
    logbuf: String,
    submit_buf_state: TypedWidgetHandle<Textbox>,
    submit_buf_submitted: WidgetEventPortHandle<TextboxSubmitted>,
    combo_typed_state: TypedWidgetHandle<Combo>,
    combo_submitted: WidgetEventPortHandle<ComboSubmitted>,
    combo_item_states: [TypedWidgetHandle<ListItem>; 4],
    combo_item_submitted: [WidgetEventPortHandle<ListItemSubmitted>; 4],
    style_color_swatch_states: [TypedWidgetHandle<ColorSwatch>; 16],
    window_info_value_states: [TypedWidgetHandle<ListItem>; 3],
    style: Style,

    demo_root: WindowHandle,
    combo_popup_root: PopupHandle,
    popup_root: PopupHandle,

    /// Shared camera and presentation state consumed by the grid widget and render callback.
    grid_3d_state: Rc<RefCell<Grid3dState>>,
    /// Fullscreen layer-zero window whose rectangle follows the platform drawable area.
    grid_root: WindowHandle,
    /// Concrete checked item reflecting whether unit-spaced grid lines are enabled.
    grid_show_minor_lines_item: MenuItemHandle,

    /// Concrete Open item whose enabled state follows file-dialog activity.
    menu_open_file: MenuItemHandle,
    /// Concrete check item reflecting log auto-scroll state.
    menu_auto_scroll_item: MenuItemHandle,
    /// First concrete item in the spacing radio pair.
    menu_comfortable_spacing: MenuItemHandle,
    /// Second concrete item in the spacing radio pair.
    menu_compact_spacing: MenuItemHandle,
    /// Whether log writes should keep the newest output visible.
    menu_auto_scroll: bool,
    file_dialog: FileDialog,
    fps: f32,
    last_frame: Instant,

    submit_button_submitted: WidgetEventPortHandle<ButtonSubmitted>,
    log_text_state: TypedWidgetHandle<TextBlock>,
    log_scroll_state: TypedWidgetHandle<ScrollArea>,
    test_button_submitted: [WidgetEventPortHandle<ButtonSubmitted>; 6],
    tree_button_submitted: [WidgetEventPortHandle<ButtonSubmitted>; 6],
    popup_button_submitted: [WidgetEventPortHandle<ButtonSubmitted>; 2],
    stack_direction_button_submitted: [WidgetEventPortHandle<ButtonSubmitted>; 6],
    weight_button_submitted: [WidgetEventPortHandle<ButtonSubmitted>; 9],
    triangle_data: Rc<RefCell<TriangleState>>,
    background_swatch_state: TypedWidgetHandle<ColorSwatch>,
}

impl State {
    pub fn new(_backend: BackendInitContext, ctx: &mut Context<SelectedBackend, Self>) -> Self {
        #[cfg(any(feature = "builder", feature = "png_source"))]
        let image_texture = load_external_image_texture(ctx);
        #[cfg(not(any(feature = "builder", feature = "png_source")))]
        let image_texture = None;
        let white_uv = {
            let atlas = ctx.renderer().atlas();
            let rect = atlas.get_icon_rect(WHITE_ICON);
            let dim = atlas.get_texture_dimension();
            let rect_min = Vec2f::new(rect.x as f32, rect.y as f32);
            let rect_extent = Vec2f::new(rect.width as f32, rect.height as f32);
            let texture_extent = Vec2f::new(dim.width as f32, dim.height as f32);
            (rect_min + rect_extent * 0.5) / texture_extent
        };

        // The fullscreen grid is rendered through the same backend-neutral colored-triangle path
        // used by the standalone cube example. The retained input widget and paint callback share
        // one View3D so arcball and wheel changes are consumed by the very next rendered frame.
        let grid_3d_state = Rc::new(RefCell::new(Grid3dState {
            view_3d: create_grid_view_3d(),
            show_minor_lines: true,
        }));
        let grid_renderer = {
            let grid_3d_state = grid_3d_state.clone();
            ctx.register_custom_renderer(move |frame: &mut SelectedFrame<'_>, args: CustomRenderArgs| {
                let area = area_from_args(&args);
                if area.rect.width <= 0 || area.rect.height <= 0 {
                    frame.enqueue_colored_vertices(area, Vec::new());
                    return;
                }
                let mut grid = grid_3d_state.borrow_mut();
                // Rendering can precede input after a host resize, so keep the projection's
                // aspect authoritative even when no pointer event has reached the widget yet.
                grid.view_3d.set_dimension(Dimensioni::new(area.rect.width, area.rect.height));
                let pvm = grid.view_3d.pvm();
                let show_minor_lines = grid.show_minor_lines;
                drop(grid);
                let vertices = build_xy_grid_vertices(area.rect, white_uv, show_minor_lines, pvm);
                frame.enqueue_colored_vertices(area, vertices);
            })
            .expect("register fullscreen X-Y grid renderer")
        };

        let triangle_data = Rc::new(RefCell::new(TriangleState { angle: 0.0 }));
        let suzanne_path = demo_asset_path("assets/suzanne.obj");
        let suzanne_bytes = fs::read(&suzanne_path).unwrap_or_else(|err| panic!("Failed to read {}: {err}", suzanne_path.display()));
        let pm_suzanne = Obj::from_byte_stream(suzanne_bytes.as_slice())
            .unwrap_or_else(|err| panic!("Failed to parse {}: {err}", suzanne_path.display()))
            .to_polymesh();
        let bounds = pm_suzanne.calculate_bounding_box();
        let mesh_buffers = build_mesh_buffers(&pm_suzanne);
        let view_3d = View3D::new(
            Camera::new(
                bounds.center(),
                bounds.max.length() * 3.0,
                Quat::identity(),
                PI / 4.0,
                1.0,
                0.1,
                bounds.max.length() * 10.0,
            ),
            Dimension::new(600, 600),
            bounds,
        );
        let suzanne_data = Rc::new(RefCell::new(SuzanneData { view_3d, mesh: mesh_buffers }));

        let triangle_renderer = {
            let triangle_data = triangle_data.clone();
            ctx.register_custom_renderer(move |frame: &mut SelectedFrame<'_>, args: CustomRenderArgs| {
                let triangle = triangle_data.borrow();
                let area = area_from_args(&args);
                frame.enqueue_colored_vertices(area, build_triangle_vertices(area.rect, white_uv, triangle.angle));
            })
            .expect("register triangle renderer")
        };
        let suzanne_renderer = {
            let suzanne_data = suzanne_data.clone();
            ctx.register_custom_renderer(move |frame: &mut SelectedFrame<'_>, args: CustomRenderArgs| {
                let area = area_from_args(&args);
                let mut suzanne = suzanne_data.borrow_mut();
                // Projection is a rendering cache derived from the authoritative committed area.
                // This also initializes the aspect ratio on an inputless first frame.
                suzanne.view_3d.set_dimension(Dimensioni::new(area.rect.width, area.rect.height));
                frame.enqueue_mesh_draw(
                    area,
                    MeshSubmission {
                        mesh: suzanne.mesh.clone(),
                        pvm: suzanne.view_3d.pvm(),
                        view_model: suzanne.view_3d.view_matrix(),
                    },
                );
            })
            .expect("register Suzanne renderer")
        };
        let red_texture = upload_solid_texture(ctx, 64, 64, [0xFF, 0, 0, 0xFF]);
        let green_texture = upload_solid_texture(ctx, 24, 32, [0, 0xFF, 0, 0xFF]);
        let blue_texture = upload_solid_texture(ctx, 64, 24, [0, 0, 0xFF, 0xFF]);
        let noise_texture = upload_noise_texture(ctx, 24, 32);
        let texture_buttons = [
            retained_leaf::<ButtonBuilder>(ButtonParameters::with_image(
                "Texture 1 - Red",
                Some(red_texture),
                WidgetOption::FRAME,
                WidgetFillOption::ALL,
            )),
            retained_leaf::<ButtonBuilder>(ButtonParameters::with_image(
                "Texture 2 - Green",
                Some(green_texture),
                WidgetOption::FRAME,
                WidgetFillOption::ALL,
            )),
            retained_leaf::<ButtonBuilder>(ButtonParameters::with_image(
                "Texture 3 - Blue",
                Some(blue_texture),
                WidgetOption::FRAME,
                WidgetFillOption::ALL,
            )),
            retained_leaf::<ButtonBuilder>(ButtonParameters::with_image(
                "Texture 4 - Noise",
                Some(noise_texture),
                WidgetOption::FRAME,
                WidgetFillOption::ALL,
            )),
        ];
        let external_image_button = image_texture.map(|texture| {
            retained_leaf::<ButtonBuilder>(ButtonParameters::with_scaled_image(
                "External Image",
                Some(texture),
                WidgetOption::FRAME,
                WidgetFillOption::ALL,
            ))
        });
        let style_color_slider_pairs = std::array::from_fn(|_| {
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                255.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            ))
        });
        let style_color_slider_states = style_color_slider_pairs.each_ref().map(|(state, _)| state.clone());
        let style_color_slider_changed = style_color_slider_pairs.each_ref().map(|(handle, _)| handle.changed());
        let style_color_sliders = style_color_slider_pairs.map(|(_, runtime)| runtime);
        let style_color_swatch_pairs = std::array::from_fn(|_| stateful_leaf::<ColorSwatchBuilder>(ColorSwatchParameters::new(color(0, 0, 0, 0xFF))));
        let style_color_swatch_states = style_color_swatch_pairs.each_ref().map(|(state, _)| state.clone());
        let style_color_swatches = style_color_swatch_pairs.map(|(_, runtime)| runtime);
        let style_value_slider_pairs = [
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                16.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            )),
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                16.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            )),
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                128.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            )),
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                128.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            )),
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                128.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            )),
        ];
        let style_value_slider_states = style_value_slider_pairs.each_ref().map(|(state, _)| state.clone());
        let style_value_slider_changed = style_value_slider_pairs.each_ref().map(|(handle, _)| handle.changed());
        let style_value_sliders = style_value_slider_pairs.map(|(_, runtime)| runtime);
        let bg_slider_pairs = std::array::from_fn(|_| {
            stateful_leaf::<SliderBuilder>(SliderParameters::with_opt(
                0.0,
                0.0,
                255.0,
                0.0,
                0,
                WidgetOption::FRAME | WidgetOption::ALIGN_CENTER,
            ))
        });
        let bg_slider_states = bg_slider_pairs.each_ref().map(|(state, _)| state.clone());
        let bg_slider_changed = bg_slider_pairs.each_ref().map(|(handle, _)| handle.changed());
        let bg_sliders = bg_slider_pairs.map(|(_, runtime)| runtime);
        let text_area = TextArea::create(
            TextAreaParameters::new(
                "This is a multi-line TextArea.\nYou can type, scroll, and resize the window.\n\nTry adding more lines to see the scrollbars.",
            )
            .wrap(TextWrap::Word),
        )
        .1;
        let (submit_buf_state, submit_buf) = stateful_leaf::<TextboxBuilder>(TextboxParameters::new("").font(FontRole::Mono.into()));
        let submit_buf_submitted = submit_buf_state.submitted();
        let (log_text_state, log_text) = stateful_leaf::<TextBlockBuilder>(TextBlockParameters::new("").font(FontRole::Mono.into()));
        let (_, log_scroll_content) = Linear::create(LinearParameters::vertical([log_text]));
        let (log_scroll_state, log_scroll) = ScrollArea::create(ScrollAreaParameters::new(
            ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
            log_scroll_content,
        ));
        let typography_heading = retained_leaf::<TextBlockBuilder>(TextBlockParameters::new("NORMAL.ttf at 18px").font(FontRole::Heading.into()));
        let typography_body = retained_leaf::<TextBlockBuilder>(
            TextBlockParameters::with_wrap(
                "NORMAL.ttf at 12px remains the control font. Window titles use BOLD.ttf, and the log window uses CONSOLE.ttf for input and output.",
                TextWrap::Word,
            )
            .font(FontRole::Body.into()),
        );
        let style = Style::default().with_named_fonts(&ctx.renderer().atlas());
        let (demo_content, demo_node) = root_content();
        let (style_content, style_node) = root_content();
        let (log_content, log_node) = root_content();
        let (combo_content, combo_node) = root_content();
        let (popup_content, popup_node) = root_content();
        let (typography_content, typography_node) = root_content();
        let (triangle_content, triangle_node) = root_content();
        let (painter_content, painter_node) = root_content();
        let (falloff_content, falloff_node) = root_content();
        let (suzanne_content, suzanne_node) = root_content();
        let (stack_direction_content, stack_direction_node) = root_content();
        let (weight_content, weight_node) = root_content();
        let root_contents = DemoRootContents {
            style: style_content,
            log: log_content,
            typography: typography_content,
            triangle: triangle_content,
            painter: painter_content,
            falloff: falloff_content,
            suzanne: suzanne_content,
            stack_direction: stack_direction_content,
            weight: weight_content,
            demo: demo_content,
            combo: combo_content,
            popup: popup_content,
        };

        // The grid Window owns its menu bar and custom-render body directly. Its placeholder extent
        // is replaced from real drawable dimensions by `sync_grid_surface` every frame because this
        // construction callback cannot yet observe the platform window size.
        let (grid_menu_bar, grid_menu_items) = grid_menu_bar(ctx);
        let grid_node = Node::custom_render(
            Grid3dWidgetBuilder::create_widget(Grid3dWidgetParameters { data: grid_3d_state.clone() }),
            grid_renderer,
        );
        let grid_root = ctx.ui().create_window(
            Window::new("X-Y Grid Surface", rect(0, 0, 1, 1), grid_node)
                .menu_bar(grid_menu_bar)
                .child_window_clip(ChildWindowClip::Content),
        );
        // This dedicated desktop-like window is the layer-0 family root. Its custom-render body is
        // recorded first, every floating child is clipped to that body, and the intrinsic grid menu
        // is recorded and hit-tested last so it remains above the complete child family.
        ctx.ui()
            .set_window_layer(&grid_root, MIN_LAYER)
            .expect("grid window must accept the bottom application layer");
        ctx.ui()
            .set_window_options(
                &grid_root,
                WindowOption::NO_TITLE | WindowOption::NO_CLOSE | WindowOption::NO_RESIZE | WindowOption::NO_PADDING,
            )
            .expect("grid window must accept fullscreen chrome options");

        // Preserve the original Demo Window as a movable and resizable structural child. All demo
        // windows inherit the grid root's layer but retain screen-space geometry and sibling order.
        let (menu_bar, menu_items) = demo_menu_bar(ctx);
        let demo_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Demo Window", rect(40, 40, 300, 450), demo_node).menu_bar(menu_bar))
            .expect("grid root must own the demo window");
        let _style_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Style Editor", rect(350, 250, 300, 240), style_node))
            .expect("grid root must own the style editor");
        let _log_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Log Window", rect(350, 40, 300, 200), log_node))
            .expect("grid root must own the log window");
        let combo_popup_root = ctx
            .ui()
            .create_popup(&demo_root, "Combo Box Popup", combo_node)
            .expect("demo window must own the combo popup");
        ctx.ui()
            .set_popup_options(
                &combo_popup_root,
                WindowOption::FRAME | WindowOption::AUTO_HEIGHT | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
            )
            .expect("combo popup definition must exist");
        let popup_root = ctx
            .ui()
            .create_popup(&demo_root, "Test Popup", popup_node)
            .expect("demo window must own the test popup");
        ctx.ui()
            .set_popup_options(
                &popup_root,
                WindowOption::FRAME | WindowOption::AUTO_SIZE | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
            )
            .expect("test popup definition must exist");
        let _typography_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Typography Demo", rect(40, 500, 300, 170), typography_node))
            .expect("grid root must own the typography demo");
        let _triangle_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Triangle Window", rect(200, 100, 200, 200), triangle_node))
            .expect("grid root must own the triangle window");
        let _painter_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Painter Window", rect(820, 40, 280, 240), painter_node))
            .expect("grid root must own the painter window");
        let _falloff_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Brush Falloff", rect(820, 300, 320, 260), falloff_node))
            .expect("grid root must own the brush-falloff window");
        let _suzanne_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Suzanne Window", rect(220, 220, 300, 300), suzanne_node))
            .expect("grid root must own the Suzanne window");
        let _stack_direction_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Stack Direction Demo", rect(530, 40, 280, 220), stack_direction_node))
            .expect("grid root must own the stack-direction demo");
        let _weight_root = ctx
            .ui()
            .create_child_window(&grid_root, Window::new("Weight Demo", rect(530, 270, 280, 260), weight_node))
            .expect("grid root must own the weight demo");
        let (combo_typed_state, combo_runtime) = stateful_leaf::<ComboBuilder>(ComboParameters::new());
        let combo_submitted = combo_typed_state.submitted();
        let combo_item_pairs = [
            stateful_leaf::<ListItemBuilder>(ListItemParameters::new("Apple")),
            stateful_leaf::<ListItemBuilder>(ListItemParameters::new("Banana")),
            stateful_leaf::<ListItemBuilder>(ListItemParameters::new("Cherry")),
            stateful_leaf::<ListItemBuilder>(ListItemParameters::new("Date")),
        ];
        let combo_item_states = combo_item_pairs.each_ref().map(|(state, _)| state.clone());
        let combo_item_submitted = combo_item_pairs.each_ref().map(|(handle, _)| handle.submitted());
        let combo_labels: Vec<String> = combo_item_states
            .iter()
            .map(|item| item.try_read(|item| item.label().to_owned()).expect("combo item state unavailable"))
            .collect();
        // Seed the retained combo once from the statically constructed demo items. Selection changes
        // remain widget-owned; no frame callback needs to recopy these unchanged labels.
        combo_typed_state
            .try_update(|combo| combo.update_items(&combo_labels))
            .expect("combo state unavailable");
        let combo_items = combo_item_pairs.map(|(_, runtime)| runtime);
        let window_info_value_pairs =
            ["40, 40", "300, 450", "0.0"].map(|label| stateful_leaf::<ListItemBuilder>(ListItemParameters::with_opt(label, WidgetOption::NO_INTERACT)));
        let window_info_value_states = window_info_value_pairs.each_ref().map(|(state, _)| state.clone());
        let window_info_values = window_info_value_pairs.map(|(_, runtime)| runtime);
        let (submit_button_submitted, submit_button) = centered_button("Submit");
        let typography_button = centered_button("Control Preview").1;
        let test_button_pairs = [
            centered_button("Button 1"),
            centered_button("Button 2"),
            centered_button("Button 3"),
            centered_button("Popup"),
            centered_button("Button 4"),
            centered_button("Dialog"),
        ];
        let test_button_submitted = test_button_pairs.each_ref().map(|(submitted, _)| submitted.clone());
        let test_buttons = test_button_pairs.map(|(_, runtime)| runtime);
        let tree_button_pairs = [
            centered_button("Button 1"),
            centered_button("Button 2"),
            centered_button("Button 3"),
            centered_button("Button 4"),
            centered_button("Button 5"),
            centered_button("Button 6"),
        ];
        let tree_button_submitted = tree_button_pairs.each_ref().map(|(submitted, _)| submitted.clone());
        let tree_buttons = tree_button_pairs.map(|(_, runtime)| runtime);
        let popup_button_pairs = [centered_button("Hello"), centered_button("World")];
        let popup_button_submitted = popup_button_pairs.each_ref().map(|(submitted, _)| submitted.clone());
        let popup_buttons = popup_button_pairs.map(|(_, runtime)| runtime);
        let stack_direction_button_pairs = [
            centered_button("Call 1"),
            centered_button("Call 2"),
            centered_button("Call 3"),
            centered_button("Call 1"),
            centered_button("Call 2"),
            centered_button("Call 3"),
        ];
        let stack_direction_button_submitted = stack_direction_button_pairs.each_ref().map(|(submitted, _)| submitted.clone());
        let stack_direction_buttons = stack_direction_button_pairs.map(|(_, runtime)| runtime);
        let weight_button_pairs = [
            centered_button("w1"),
            centered_button("w2"),
            centered_button("w3"),
            centered_button("g1"),
            centered_button("g2"),
            centered_button("g3"),
            centered_button("g4"),
            centered_button("g5"),
            centered_button("g6"),
        ];
        let weight_button_submitted = weight_button_pairs.each_ref().map(|(submitted, _)| submitted.clone());
        let weight_buttons = weight_button_pairs.map(|(_, runtime)| runtime);
        let (background_swatch_state, background_swatch) = stateful_leaf::<ColorSwatchBuilder>(ColorSwatchParameters::new(color(90, 95, 100, 0xFF)));
        let runtimes = DemoRuntimes {
            bg_sliders,
            style_color_sliders,
            style_value_sliders,
            submit_buf,
            text_area,
            combo: combo_runtime,
            combo_items,
            style_color_labels: [
                static_label("text"),
                static_label("border:"),
                static_label("windowbg:"),
                static_label("titlebg:"),
                static_label("titletext:"),
                static_label("panelbg:"),
                static_label("button:"),
                static_label("buttonhover:"),
                static_label("base:"),
                static_label("basehover:"),
                static_label("scrollbase:"),
                static_label("scrollthumb:"),
                static_label("focus:"),
                static_label("window focus:"),
                static_label("menu foreground:"),
                static_label("menu background:"),
            ],
            style_color_swatches,
            style_metric_labels: [
                static_label("padding"),
                static_label("spacing"),
                static_label("title height"),
                static_label("thumb size"),
                static_label("scroll size"),
            ],
            stack_direction_labels: [static_label("Top -> Bottom"), static_label("Bottom -> Top")],
            weight_labels: [static_label("Row weights 1 : 2 : 3"), static_label("Grid weights rows 1 : 2")],
            window_info_labels: [static_label("Position:"), static_label("Size:"), static_label("FPS:")],
            window_info_values,
            test_button_labels: [
                static_label("Test buttons 1:"),
                static_label("Test buttons 2:"),
                static_label("Test buttons 3:"),
            ],
            tree_labels: [static_label("Hello"), static_label("world")],
            background_labels: [static_label("Red:"), static_label("Green:"), static_label("Blue:")],
            submit_button,
            log_scroll,
            typography_heading,
            typography_body,
            typography_button,
            test_buttons,
            tree_buttons,
            popup_buttons,
            texture_buttons,
            stack_direction_buttons,
            weight_buttons,
            external_image_button,
            checkboxes: [
                Checkbox::create(CheckboxParameters::new("Checkbox 1", false)).1,
                Checkbox::create(CheckboxParameters::new("Checkbox 2", true)).1,
                Checkbox::create(CheckboxParameters::new("Checkbox 3", false)).1,
            ],
            triangle_renderer,
            suzanne_renderer,
            triangle_widget: Custom::create(CustomParameters::new("Triangle")),
            painter_widget: PainterDemoBuilder::create_widget(PainterDemoParameters),
            falloff_widget: FalloffEditorBuilder::create_widget(FalloffEditorParameters),
            suzanne_widget: SuzanneWidgetBuilder::create_widget(SuzanneWidgetParameters { data: suzanne_data.clone() }),
            background_swatch,
        };
        // The file picker is a library component owned by this application state. Its dialog and
        // controls use the same generic Context APIs and dispatcher as the rest of the demo.
        let file_dialog = FileDialog::new(ctx, &demo_root, Self::file_dialog_mut);
        let mut state = Self {
            bg: [90.0, 95.0, 100.0],
            bg_slider_states,
            bg_slider_changed,
            style_color_slider_states,
            style_color_slider_changed,
            style_value_slider_states,
            style_value_slider_changed,
            logbuf: String::new(),
            submit_buf_state,
            submit_buf_submitted,
            combo_typed_state,
            combo_submitted,
            combo_item_states,
            combo_item_submitted,
            style_color_swatch_states,
            window_info_value_states,
            style,
            demo_root,
            combo_popup_root,
            popup_root,
            grid_3d_state,
            grid_root,
            grid_show_minor_lines_item: grid_menu_items.show_minor_lines,
            menu_open_file: menu_items.open_file,
            menu_auto_scroll_item: menu_items.auto_scroll,
            menu_comfortable_spacing: menu_items.comfortable_spacing,
            menu_compact_spacing: menu_items.compact_spacing,
            menu_auto_scroll: true,
            file_dialog,
            fps: 0.0,
            last_frame: Instant::now(),
            submit_button_submitted,
            log_text_state,
            log_scroll_state,
            test_button_submitted,
            tree_button_submitted,
            popup_button_submitted,
            stack_direction_button_submitted,
            weight_button_submitted,
            triangle_data,
            background_swatch_state,
        };
        state.sync_background_controls_from_bg();
        state.sync_style_controls_from_style();
        state.build_root_contents(runtimes, root_contents);
        state
    }

    fn subscribe_events(&self, context: &mut Context<SelectedBackend, Self>) {
        for (index, changed) in self.bg_slider_changed.iter().enumerate() {
            context.subscribe_with(changed.clone(), index, Self::background_changed).unwrap();
        }
        for (index, changed) in self.style_color_slider_changed.iter().enumerate() {
            context.subscribe_with(changed.clone(), index, Self::style_color_changed).unwrap();
        }
        for (index, changed) in self.style_value_slider_changed.iter().enumerate() {
            context.subscribe_with(changed.clone(), index, Self::style_value_changed).unwrap();
        }

        context.subscribe(self.submit_buf_submitted.clone(), Self::text_submitted).unwrap();
        context.subscribe(self.submit_button_submitted.clone(), Self::submit_button).unwrap();
        for (index, submitted) in self.test_button_submitted.iter().enumerate() {
            context.subscribe_context_with(submitted.clone(), index, Self::test_button).unwrap();
        }
        for (submitted, label) in self.tree_button_submitted.iter().zip([
            "Pressed button 1",
            "Pressed button 2",
            "Pressed button 3",
            "Pressed button 4",
            "Pressed button 5",
            "Pressed button 6",
        ]) {
            context.subscribe_with(submitted.clone(), label, Self::log_button).unwrap();
        }
        context.subscribe_context(self.combo_submitted.clone(), Self::combo_submitted).unwrap();
        for (index, submitted) in self.combo_item_submitted.iter().enumerate() {
            context.subscribe_context_with(submitted.clone(), index, Self::combo_item).unwrap();
        }
        context.subscribe(self.combo_popup_root.events(), Self::combo_popup_event).unwrap();
        // One concrete window event stream reports both geometry changes and close requests. The
        // floating Demo Window retains diagnostics independently of the platform-sized grid window.
        context.subscribe_context(self.demo_root.events(), Self::demo_window_event).unwrap();
        for (submitted, label) in self.popup_button_submitted.iter().zip(["Hello", "World"]) {
            context.subscribe_with(submitted.clone(), label, Self::log_button).unwrap();
        }
        for (submitted, label) in self.stack_direction_button_submitted.iter().zip([
            "Top->Bottom: call 1",
            "Top->Bottom: call 2",
            "Top->Bottom: call 3",
            "Bottom->Top: call 1",
            "Bottom->Top: call 2",
            "Bottom->Top: call 3",
        ]) {
            context.subscribe_with(submitted.clone(), label, Self::log_button).unwrap();
        }
        for (submitted, label) in self.weight_button_submitted.iter().zip([
            "Weight row: 1",
            "Weight row: 2",
            "Weight row: 3",
            "Weight grid: 1",
            "Weight grid: 2",
            "Weight grid: 3",
            "Weight grid: 4",
            "Weight grid: 5",
            "Weight grid: 6",
        ]) {
            context.subscribe_with(submitted.clone(), label, Self::log_button).unwrap();
        }
        // Subscribe to this application-owned component's stable completion source once.
        let file_dialog_completed = self.file_dialog.completed();
        context.subscribe_context(file_dialog_completed, Self::file_dialog_completed).unwrap();
    }

    fn background_changed(&mut self, index: &usize, event: &SliderChanged) {
        self.bg[*index] = event.value;
        self.sync_background_swatch();
    }

    fn style_color_changed(&mut self, index: &usize, event: &SliderChanged) {
        let color_index = *index / 4;
        let color = match color_index {
            0..=11 => &mut self.style.colors[color_index],
            12 => &mut self.style.focus_color,
            13 => &mut self.style.window_focus_color,
            14 => &mut self.style.menu_foreground,
            15 => &mut self.style.menu_background,
            _ => return,
        };
        let value = event.value as u8;
        match *index % 4 {
            0 => color.r = value,
            1 => color.g = value,
            2 => color.b = value,
            _ => color.a = value,
        }
    }

    fn style_value_changed(&mut self, index: &usize, event: &SliderChanged) {
        match index {
            0 => self.style.padding = event.value as i32,
            1 => self.style.spacing = event.value as i32,
            2 => self.style.title_height = event.value as i32,
            3 => self.style.thumb_size = event.value as i32,
            4 => self.style.scrollbar_size = event.value as i32,
            _ => unreachable!("style value slider index is bounded by construction"),
        }
    }

    fn text_submitted(&mut self, event: &TextboxSubmitted) {
        self.submit_log(event.text.clone());
    }

    fn submit_button(&mut self, _: &ButtonSubmitted) {
        let text = self
            .submit_buf_state
            .try_read(|submit_buf| submit_buf.text().to_owned())
            .expect("submit textbox state unavailable");
        self.submit_log(text);
    }

    fn test_button(&mut self, index: &usize, context: &mut Ui<'_>, _: &ButtonSubmitted) {
        match index {
            0 => self.write_log("Pressed button 1"),
            1 => self.write_log("Pressed button 2"),
            2 => self.write_log("Pressed button 3"),
            3 => {
                // Apply the popup request at the typed-event boundary. WindowManager owns placement,
                // exclusivity, and the layout commit; State needs no frame-polled command flag.
                context
                    .show_popup(&self.popup_root)
                    .expect("test popup definition and owning demo window must exist");
            }
            4 => self.write_log("Pressed button 4"),
            5 if !self.file_dialog.is_open() => {
                self.file_dialog.open(context, FileDialogRequest::default());
                self.write_log("Open dialog!");
            }
            5 => {}
            _ => unreachable!("test button index is bounded by construction"),
        }
    }

    fn log_button(&mut self, label: &&'static str, _: &ButtonSubmitted) {
        self.write_log(label);
    }

    fn combo_submitted(&mut self, context: &mut Ui<'_>, event: &ComboSubmitted) {
        // Combo owns the semantic toggle; its demo window owns the retained popup definition.
        // Reconcile them once, at the event boundary that joins the two application-chosen pieces.
        if event.open {
            // The submission owns the geometry from the update that routed this click, so opening
            // needs neither a widget-state read nor a previous-frame anchor snapshot.
            context
                .show_popup_at(&self.combo_popup_root, event.anchor)
                .expect("combo popup definition and owning demo window must exist");
        } else {
            context.hide_popup(&self.combo_popup_root).expect("combo popup definition must exist");
        }
    }

    fn combo_item(&mut self, index: &usize, context: &mut Ui<'_>, _: &ListItemSubmitted) {
        let labels: Vec<String> = self
            .combo_item_states
            .iter()
            .map(|item| item.try_read(|item| item.label().to_owned()).expect("combo item state unavailable"))
            .collect();
        let selected = self
            .combo_typed_state
            .try_update(|combo| combo.select(*index, &labels))
            .expect("combo state unavailable");
        // Selection closes both authorities in the same dispatch transaction: Combo commits its
        // semantic state above, and the context hides the retained popup before the next layout.
        context.hide_popup(&self.combo_popup_root).expect("combo popup definition must exist");
        if let Some(label) = selected {
            self.write_log(format!("Selected: {label}").as_str());
        }
    }

    fn combo_popup_event(&mut self, event: &PopupEvent) {
        // Dismissed covers every policy-driven hide, including outside presses, replacement, and
        // recursive forest hiding. Reflect that typed fact into the composed Combo so its
        // next header click opens instead of toggling stale semantic state closed.
        if matches!(event, PopupEvent::Dismissed) {
            self.combo_typed_state.try_update(Combo::close_popup).expect("combo state unavailable");
        }
    }

    /// Starts a fresh log session for the registered New Session item.
    fn menu_new_session(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.clear_log();
        self.write_log("Started a new demo session");
    }

    /// Opens the independent file dialog for the registered Open item.
    fn menu_open_file(&mut self, context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        if self.file_dialog.is_open() {
            return;
        }
        context.menu_item_mut(&self.menu_open_file).expect("Open menu item unavailable").enabled = false;
        self.file_dialog.open(context, FileDialogRequest::default());
        self.write_log("Opened the file dialog from File > Open...");
    }

    /// Clears log output for the registered Clear Log item.
    fn menu_clear_log(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.clear_log();
    }

    /// Toggles auto-scroll and updates this concrete item's check marker.
    fn menu_toggle_auto_scroll(&mut self, context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.menu_auto_scroll = !self.menu_auto_scroll;
        context
            .menu_item_mut(&self.menu_auto_scroll_item)
            .expect("auto-scroll menu item unavailable")
            .mark = MenuItemMark::Checked(self.menu_auto_scroll);
        self.write_log(if self.menu_auto_scroll {
            "Enabled log auto-scroll"
        } else {
            "Disabled log auto-scroll"
        });
    }

    /// Selects comfortable spacing through its registered concrete item.
    fn menu_comfortable_spacing(&mut self, context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.select_menu_spacing(context, true, 4);
    }

    /// Selects compact spacing through its registered concrete item.
    fn menu_compact_spacing(&mut self, context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.select_menu_spacing(context, false, 1);
    }

    /// Writes application information for the registered About item.
    fn menu_about(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.write_log("microui-redux retained-mode full demo with per-window menus");
    }

    /// Restores the fullscreen grid's documented initial camera orientation and distance.
    fn grid_reset_view(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        // Replace the complete view rather than attempting to reverse an accumulated quaternion;
        // this also restores projection and zoom bounds-derived state in one authoritative value.
        self.grid_3d_state.borrow_mut().view_3d = create_grid_view_3d();
        self.write_log("Reset X-Y grid arcball view");
    }

    /// Toggles unit-spaced geometry from the fullscreen grid's own checked menu item.
    fn grid_toggle_minor_lines(&mut self, context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        // End the shared-state borrow before updating the item and log, keeping callback-owned data
        // independent from the rest of mutable application state.
        let show_minor_lines = {
            let mut grid = self.grid_3d_state.borrow_mut();
            grid.show_minor_lines = !grid.show_minor_lines;
            grid.show_minor_lines
        };
        context
            .menu_item_mut(&self.grid_show_minor_lines_item)
            .expect("minor-grid-lines menu item unavailable")
            .mark = MenuItemMark::Checked(show_minor_lines);
        self.write_log(if show_minor_lines {
            "Enabled minor X-Y grid lines"
        } else {
            "Showing major X-Y grid lines only"
        });
    }

    /// Describes the separate background window from its own Help menu.
    fn grid_about(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.write_log("Layer-0 fullscreen X-Y grid: left-drag to orbit and use the wheel to zoom");
    }

    /// Updates the two concrete spacing markers and applies the chosen style value.
    fn select_menu_spacing(&mut self, context: &mut Ui<'_>, comfortable: bool, spacing: i32) {
        context
            .menu_item_mut(&self.menu_comfortable_spacing)
            .expect("comfortable-spacing menu item unavailable")
            .mark = MenuItemMark::Radio(comfortable);
        context
            .menu_item_mut(&self.menu_compact_spacing)
            .expect("compact-spacing menu item unavailable")
            .mark = MenuItemMark::Radio(!comfortable);
        self.style.spacing = spacing;
        set_slider_value(&self.style_value_slider_states[1], spacing as Real);
        self.write_log(if comfortable {
            "Selected comfortable control spacing"
        } else {
            "Selected compact control spacing"
        });
    }

    /// Handles all manager-originated events for the ordinary floating Demo Window.
    fn demo_window_event(&mut self, context: &mut Ui<'_>, event: &WindowEvent) {
        let WindowEvent::GeometryChanged { rect: event_rect } = event else {
            // The demo intentionally leaves close requests observational; host shutdown policy is
            // owned by the shared runner rather than inferred from a geometry notification.
            return;
        };
        // Clamp the demo-specific minimum at the user move/resize boundary without coupling it to
        // the fullscreen grid geometry.
        let mut rect = *event_rect;
        rect.width = rect.width.max(240);
        rect.height = rect.height.max(300);
        if (rect.width, rect.height) != (event_rect.width, event_rect.height) {
            context.set_window_rect(&self.demo_root, rect).expect("demo window must exist");
        }

        // These retained values describe the floating Demo Window, not the platform-sized grid.
        let [value_pos, value_size, _] = &self.window_info_value_states;
        value_pos
            .try_update(|value| value.set_label(format!("{}, {}", rect.x, rect.y)))
            .expect("window position state unavailable");
        value_size
            .try_update(|value| value.set_label(format!("{}, {}", rect.width, rect.height)))
            .expect("window size state unavailable");
    }

    fn submit_log(&mut self, text: String) {
        self.write_log(text.as_str());
        self.submit_buf_state.try_update(Textbox::clear).expect("submit textbox unavailable");
    }

    fn sync_background_controls_from_bg(&mut self) {
        set_slider_value(&self.bg_slider_states[0], self.bg[0]);
        set_slider_value(&self.bg_slider_states[1], self.bg[1]);
        set_slider_value(&self.bg_slider_states[2], self.bg[2]);
        self.sync_background_swatch();
    }

    fn sync_background_swatch(&mut self) {
        let fill = color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255);
        self.background_swatch_state
            .try_update(|swatch| {
                swatch.set_fill(fill);
                swatch.set_label(format!("#{:02X}{:02X}{:02X}", fill.r, fill.g, fill.b));
            })
            .expect("background swatch state unavailable");
    }

    fn sync_style_controls_from_style(&mut self) {
        let colors = self.style.colors.into_iter().chain([
            self.style.focus_color,
            self.style.window_focus_color,
            self.style.menu_foreground,
            self.style.menu_background,
        ]);
        for (i, color) in colors.enumerate() {
            let slider_base = i * 4;
            set_slider_value(&self.style_color_slider_states[slider_base], color.r as Real);
            set_slider_value(&self.style_color_slider_states[slider_base + 1], color.g as Real);
            set_slider_value(&self.style_color_slider_states[slider_base + 2], color.b as Real);
            set_slider_value(&self.style_color_slider_states[slider_base + 3], color.a as Real);
            self.style_color_swatch_states[i]
                .try_update(|swatch| swatch.set_fill(color))
                .expect("style swatch state unavailable");
        }
        set_slider_value(&self.style_value_slider_states[0], self.style.padding as Real);
        set_slider_value(&self.style_value_slider_states[1], self.style.spacing as Real);
        set_slider_value(&self.style_value_slider_states[2], self.style.title_height as Real);
        set_slider_value(&self.style_value_slider_states[3], self.style.thumb_size as Real);
        set_slider_value(&self.style_value_slider_states[4], self.style.scrollbar_size as Real);
    }

    fn write_log(&mut self, text: &str) {
        if !self.logbuf.is_empty() {
            self.logbuf.push('\n');
        }
        self.logbuf.push_str(text);
        self.log_text_state.set_text(self.logbuf.clone()).expect("log text state unavailable");
        if self.menu_auto_scroll {
            self.log_scroll_state
                .try_update(ScrollArea::scroll_to_end)
                .expect("log scroll area unavailable");
        }
    }

    /// Clears both the application log buffer and its retained text presentation.
    fn clear_log(&mut self) {
        self.logbuf.clear();
        self.log_text_state.set_text(String::new()).expect("log text state unavailable");
    }

    /// Adds one disclosure section using the new state-owned container path.
    ///
    /// The demo does not mutate these top-level sections externally, so their weak handles can be
    /// discarded while each retained disclosure node keeps its state alive.
    fn section(tree: &mut DemoNodes, label: &str, expanded: bool, f: impl FnOnce(&mut DemoNodes)) {
        let _ = tree.header(label, expanded, f);
    }

    fn build_root_contents(&mut self, runtimes: DemoRuntimes, roots: DemoRootContents) {
        let DemoRuntimes {
            bg_sliders,
            style_color_sliders,
            style_value_sliders,
            submit_buf,
            text_area,
            combo,
            combo_items,
            style_color_labels,
            style_color_swatches,
            style_metric_labels,
            stack_direction_labels,
            weight_labels,
            window_info_labels,
            window_info_values,
            test_button_labels,
            tree_labels,
            background_labels,
            submit_button,
            log_scroll,
            typography_heading,
            typography_body,
            typography_button,
            test_buttons,
            tree_buttons,
            popup_buttons,
            texture_buttons,
            stack_direction_buttons,
            weight_buttons,
            external_image_button,
            checkboxes,
            triangle_renderer,
            suzanne_renderer,
            triangle_widget,
            painter_widget,
            falloff_widget,
            suzanne_widget,
            background_swatch,
        } = runtimes;
        replace_root_content(
            &roots.style,
            DemoNodes::build(move |tree| {
                let color_row = [
                    TrackSize::Fixed(80),
                    TrackSize::Flex(1.0),
                    TrackSize::Flex(1.0),
                    TrackSize::Flex(1.0),
                    TrackSize::Flex(1.0),
                    TrackSize::Flex(1.0),
                ];
                let metrics_row = [TrackSize::Fixed(80), TrackSize::Flex(1.0)];

                tree.with_track(TrackSize::Flex(1.0))
                    .scroll_area(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, |tree| {
                        let mut sliders = style_color_sliders.into_iter();
                        for (label, swatch) in style_color_labels.into_iter().zip(style_color_swatches) {
                            let red = sliders.next().expect("red style slider");
                            let green = sliders.next().expect("green style slider");
                            let blue = sliders.next().expect("blue style slider");
                            let alpha = sliders.next().expect("alpha style slider");
                            tree.row(&color_row, LinearCrossSize::Content, |tree| {
                                tree.widget(label);
                                tree.widget(red);
                                tree.widget(green);
                                tree.widget(blue);
                                tree.widget(alpha);
                                tree.widget(swatch);
                            });
                        }
                        assert!(sliders.next().is_none(), "every style slider must be mounted");

                        for (label, slider) in style_metric_labels.into_iter().zip(style_value_sliders) {
                            tree.row(&metrics_row, LinearCrossSize::Content, |tree| {
                                tree.widget(label);
                                tree.widget(slider);
                            });
                        }
                    });
            }),
            "style",
        );

        replace_root_content(
            &roots.log,
            DemoNodes::build(|tree| {
                let submit_row = [TrackSize::Flex(1.0), TrackSize::Fixed(69)];
                tree.with_track(TrackSize::Flex(1.0)).widget(log_scroll);
                tree.row(&submit_row, LinearCrossSize::Content, |tree| {
                    tree.widget(submit_buf);
                    tree.widget(submit_button);
                });
            }),
            "log",
        );

        replace_root_content(
            &roots.typography,
            DemoNodes::build(move |tree| {
                tree.column(|tree| {
                    tree.widget(typography_heading);
                    tree.widget(typography_body);
                    tree.widget(typography_button);
                });
            }),
            "typography",
        );

        replace_root_content(
            &roots.triangle,
            DemoNodes::build(move |tree| {
                tree.with_track(TrackSize::Flex(1.0)).custom_render(triangle_widget, triangle_renderer);
            }),
            "triangle",
        );

        replace_root_content(
            &roots.suzanne,
            DemoNodes::build(move |tree| {
                tree.with_track(TrackSize::Flex(1.0)).custom_render(suzanne_widget, suzanne_renderer);
            }),
            "Suzanne",
        );

        replace_root_content(
            &roots.painter,
            DemoNodes::build(move |tree| {
                tree.with_track(TrackSize::Flex(1.0)).widget(painter_widget);
            }),
            "painter",
        );

        replace_root_content(
            &roots.falloff,
            DemoNodes::build(move |tree| {
                tree.with_track(TrackSize::Flex(1.0)).widget(falloff_widget);
            }),
            "falloff",
        );

        let mut bottom_stack = None;
        replace_root_content(
            &roots.stack_direction,
            DemoNodes::build(|tree| {
                let columns = [TrackSize::Flex(1.0), TrackSize::Flex(1.0)];
                let [label_top, label_bottom] = stack_direction_labels;
                let [button_top_0, button_top_1, button_top_2, button_bottom_0, button_bottom_1, button_bottom_2] = stack_direction_buttons;
                tree.row(&columns, LinearCrossSize::Content, |tree| {
                    tree.widget(label_top);
                    tree.widget(label_bottom);
                });
                tree.row(&columns, LinearCrossSize::fixed(120), |tree| {
                    tree.column(|tree| {
                        tree.fixed_column(28, false, |tree| {
                            tree.widget(button_top_0);
                            tree.widget(button_top_1);
                            tree.widget(button_top_2);
                        });
                    });
                    tree.column(|tree| {
                        bottom_stack = Some(tree.fixed_column(28, false, |tree| {
                            tree.widget(button_bottom_0);
                            tree.widget(button_bottom_1);
                            tree.widget(button_bottom_2);
                        }));
                    });
                });
            }),
            "stack direction",
        );
        bottom_stack
            .expect("bottom stack must be constructed")
            .try_update(|linear| linear.set_direction(LinearDirection::BottomToTop))
            .expect("bottom stack state unavailable");

        replace_root_content(
            &roots.weight,
            DemoNodes::build(|tree| {
                let [row_weight_label, grid_weight_label] = weight_labels;
                let [
                    button_row_0,
                    button_row_1,
                    button_row_2,
                    button_grid_0,
                    button_grid_1,
                    button_grid_2,
                    button_grid_3,
                    button_grid_4,
                    button_grid_5,
                ] = weight_buttons;
                let row = [TrackSize::Flex(1.0), TrackSize::Flex(2.0), TrackSize::Flex(3.0)];
                let cols = [TrackSize::Flex(1.0), TrackSize::Flex(1.0), TrackSize::Flex(1.0)];
                let rows = [TrackSize::Flex(1.0), TrackSize::Flex(2.0)];
                tree.row(&[TrackSize::Flex(1.0)], LinearCrossSize::Content, |tree| {
                    tree.widget(row_weight_label);
                });
                tree.row(&row, LinearCrossSize::fixed(28), |tree| {
                    tree.widget(button_row_0);
                    tree.widget(button_row_1);
                    tree.widget(button_row_2);
                });
                tree.row(&[TrackSize::Flex(1.0)], LinearCrossSize::Content, |tree| {
                    tree.widget(grid_weight_label);
                });
                // Grid owns the remaining vertical slot directly. A one-item Linear wrapper would
                // add a relationship that carries no layout meaning here.
                tree.grid(&cols, &rows, TrackSize::Flex(1.0), |tree| {
                    tree.widget(button_grid_0);
                    tree.widget(button_grid_1);
                    tree.widget(button_grid_2);
                    tree.widget(button_grid_3);
                    tree.widget(button_grid_4);
                    tree.widget(button_grid_5);
                });
            }),
            "weight",
        );

        replace_root_content(
            &roots.combo,
            DemoNodes::build(|tree| {
                for item in combo_items {
                    tree.widget(item);
                }
            }),
            "combo popup",
        );

        replace_root_content(
            &roots.popup,
            DemoNodes::build(|tree| {
                for button in popup_buttons {
                    tree.widget(button);
                }
            }),
            "test popup",
        );

        replace_root_content(
            &roots.demo,
            DemoNodes::build(|tree| {
                let window_info_row = [TrackSize::Fixed(54), TrackSize::Flex(1.0)];
                let button_widths = [TrackSize::Fixed(86), TrackSize::Flex(1.0), TrackSize::Fixed(109)];
                let tree_widths = [TrackSize::Fixed(140), TrackSize::Flex(1.0)];
                let tree_button_widths = [TrackSize::Fixed(54), TrackSize::Fixed(54)];
                let background_widths = [TrackSize::Flex(1.0), TrackSize::Fixed(77)];
                let slider_row = [TrackSize::Fixed(46), TrackSize::Flex(1.0)];
                let [label_pos, label_size, label_fps] = window_info_labels;
                let [value_pos, value_size, value_fps] = window_info_values;
                let [button0, button1, button2, button3, button4, dialog_button] = test_buttons;
                let [test_label0, test_label1, test_label2] = test_button_labels;
                let [tree_button0, tree_button1, tree_button2, tree_button3, tree_button4, tree_button5] = tree_buttons;
                let [checkbox0, checkbox1, checkbox2] = checkboxes;
                let [tree_label_hello, tree_label_world] = tree_labels;
                let [slider_red, slider_green, slider_blue] = bg_sliders;
                let [label_red, label_green, label_blue] = background_labels;
                let [texture0, texture1, texture2, texture3] = texture_buttons;

                tree.with_track(TrackSize::Flex(1.0))
                .scroll_area(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, |tree| {
                Self::section(tree, "Window Info", false, |tree| {
                    tree.row(&window_info_row, LinearCrossSize::Content, |tree| {
                        tree.widget(label_pos);
                        tree.widget(value_pos);
                    });
                    tree.row(&window_info_row, LinearCrossSize::Content, |tree| {
                        tree.widget(label_size);
                        tree.widget(value_size);
                    });
                    tree.row(&window_info_row, LinearCrossSize::Content, |tree| {
                        tree.widget(label_fps);
                        tree.widget(value_fps);
                    });
                });

                Self::section(tree, "Test Buttons", true, |tree| {
                    tree.row(&button_widths, LinearCrossSize::Content, |tree| {
                        tree.widget(test_label0);
                        tree.widget(button0);
                        tree.widget(button1);
                    });
                    tree.row(&button_widths, LinearCrossSize::Content, |tree| {
                        tree.widget(test_label1);
                        tree.widget(button2);
                        tree.widget(button3);
                    });
                    tree.row(&button_widths, LinearCrossSize::Content, |tree| {
                        tree.widget(test_label2);
                        tree.widget(button4);
                        tree.widget(dialog_button);
                    });
                });

                Self::section(tree, "Combo Box", true, |tree| {
                    tree.widget(combo);
                });

                Self::section(tree, "Tree and Text", true, |tree| {
                    tree.row(&tree_widths, LinearCrossSize::Content, |tree| {
                        tree.column(|tree| {
                            let _ = tree.tree_node("Test 1", false, |tree| {
                                let _ = tree.tree_node("Test 1a", false, |tree| {
                                    tree.widget(tree_label_hello);
                                    tree.widget(tree_label_world);
                                });
                                let _ = tree.tree_node("Test 1b", false, |tree| {
                                    tree.widget(tree_button0);
                                    tree.widget(tree_button1);
                                });
                            });
                            let _ = tree.tree_node("Test 2", false, |tree| {
                                tree.row(&tree_button_widths, LinearCrossSize::Content, |tree| {
                                    tree.widget(tree_button2);
                                    tree.widget(tree_button3);
                                });
                                tree.row(&tree_button_widths, LinearCrossSize::Content, |tree| {
                                    tree.widget(tree_button4);
                                    tree.widget(tree_button5);
                                });
                            });
                            let _ = tree.tree_node("Test 3", false, |tree| {
                                tree.widget(checkbox0);
                                tree.widget(checkbox1);
                                tree.widget(checkbox2);
                            });
                        });
                        tree.column(|tree| {
                            tree.text_with_wrap(
                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla.",
                                TextWrap::Word,
                            );
                        });
                    });
                });

                Self::section(tree, "TextArea", true, |tree| {
                    tree.with_track(TrackSize::Fixed(120)).widget(text_area);
                });

                Self::section(tree, "Background Color", true, |tree| {
                    // Let the row derive its height from the three slider rows. A fixed pixel
                    // estimate becomes stale when font metrics, control padding, or spacing
                    // changes and can place the Blue control underneath the next disclosure.
                    tree.row(&background_widths, LinearCrossSize::Content, |tree| {
                        tree.column(|tree| {
                            tree.row(&slider_row, LinearCrossSize::Content, |tree| {
                                tree.widget(label_red);
                                tree.widget(slider_red);
                            });
                            tree.row(&slider_row, LinearCrossSize::Content, |tree| {
                                tree.widget(label_green);
                                tree.widget(slider_green);
                            });
                            tree.row(&slider_row, LinearCrossSize::Content, |tree| {
                                tree.widget(label_blue);
                                tree.widget(slider_blue);
                            });
                        });
                        tree.widget(background_swatch);
                    });
                });

                Self::section(tree, "Textures", true, |tree| {
                    tree.widget(texture0);
                    tree.widget(texture1);
                    tree.widget(texture2);
                    if let Some(button) = external_image_button {
                        tree.with_fixed_cross(256).widget(button);
                    }
                    tree.widget(texture3);
                });
                });
            }),
            "demo",
        );
    }

    fn style_window(&mut self, ctx: &mut Context<SelectedBackend, Self>) {
        let colors = self.style.colors.into_iter().chain([self.style.menu_foreground, self.style.menu_background]);
        for (swatch, color) in self.style_color_swatch_states.iter().zip(colors) {
            swatch.try_update(|swatch| swatch.set_fill(color)).expect("style swatch state unavailable");
        }
        ctx.set_style(&self.style);
    }

    fn typography_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn triangle_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn suzanne_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn painter_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn falloff_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn stack_direction_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn weight_window(&mut self, _ctx: &mut Context<SelectedBackend, Self>) {}

    fn update_fps_label(&mut self) {
        // FPS is produced by this frame callback rather than discovered by polling retained UI.
        self.window_info_value_states[2]
            .try_update(|value| value.set_label(format!("{:.1}", self.fps)))
            .expect("window fps state unavailable");
    }

    fn file_dialog_mut(state: &mut Self) -> &mut FileDialog {
        &mut state.file_dialog
    }

    fn file_dialog_completed(&mut self, context: &mut Ui<'_>, event: &FileDialogCompleted) {
        // Completion makes File > Open available again regardless of acceptance or cancellation.
        context.menu_item_mut(&self.menu_open_file).expect("Open menu item unavailable").enabled = true;
        match event.status() {
            FileDialogStatus::Accepted(result) => {
                self.write_log(format!("Selected file: {}", result.file_name).as_str());
            }
            FileDialogStatus::Cancelled => {
                self.write_log("File dialog canceled");
            }
        }
    }

    /// Keeps the chromeless layer-0 grid surface exactly aligned with the drawable viewport.
    fn sync_grid_surface(&mut self, ctx: &mut Context<SelectedBackend, Self>, dimensions: Dimensioni) {
        // The platform owns drawable dimensions, while Context owns retained window geometry. Join
        // those authorities once per host frame so window resizes become visible in the second
        // update/layout commit performed by the shared example runner before painting.
        ctx.ui()
            .set_window_rect(&self.grid_root, rect(0, 0, dimensions.width, dimensions.height))
            .expect("fullscreen grid window must remain registered");
    }

    /// Applies application-owned animation, style, and viewport state between update commits.
    fn process_frame(&mut self, ctx: &mut Context<SelectedBackend, Self>, dimensions: Dimensioni) {
        // Synchronize geometry before the style and animated state changes below; all of them are
        // consumed together by the runner's post-callback retained update.
        self.sync_grid_surface(ctx, dimensions);
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if dt > 0.0 {
            let inst_fps = 1.0 / dt;
            self.fps = if self.fps == 0.0 { inst_fps } else { self.fps * 0.9 + inst_fps * 0.1 };
        }
        let mut triangle = self.triangle_data.borrow_mut();
        triangle.angle = (triangle.angle + 0.02) % (std::f32::consts::PI * 2.0);
        drop(triangle);

        self.style_window(ctx);
        self.typography_window(ctx);
        self.update_fps_label();
        self.triangle_window(ctx);
        self.painter_window(ctx);
        self.falloff_window(ctx);
        self.suzanne_window(ctx);
        self.stack_direction_window(ctx);
        self.weight_window(ctx);
    }
}

fn upload_solid_texture(ctx: &mut Context<SelectedBackend, State>, width: i32, height: i32, rgba: [u8; 4]) -> TextureId {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        pixels.extend_from_slice(&rgba);
    }
    ctx.load_image_rgba(width, height, &pixels)
}

fn upload_noise_texture(ctx: &mut Context<SelectedBackend, State>, width: i32, height: i32) -> TextureId {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let value = ((x * 73 + y * 151 + x * y * 19) & 0xFF) as u8;
            pixels.extend_from_slice(&[value, value.rotate_left(2), value.rotate_left(5), 0xFF]);
        }
    }
    ctx.load_image_rgba(width, height, &pixels)
}

fn main() {
    let atlas = atlas_assets::load_atlas();
    let mut app = Application::new(atlas, |backend: BackendInitContext, ctx| State::new(backend, ctx)).unwrap();

    app.event_loop_events(
        |state, context| state.subscribe_events(context),
        |ctx, state, dimensions| state.process_frame(ctx, dimensions),
    );
}

fn area_from_args(args: &CustomRenderArgs) -> CustomRenderArea {
    CustomRenderArea { rect: args.content_area, clip: args.view }
}

fn rect_edges(rect: Recti) -> Option<[(Vec2f, Vec2f); 4]> {
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }

    let x0 = rect.x as f32;
    let y0 = rect.y as f32;
    let x1 = (rect.x + rect.width) as f32;
    let y1 = (rect.y + rect.height) as f32;
    Some([
        (Vec2f::new(x0, y0), Vec2f::new(x1, y0)),
        (Vec2f::new(x1, y0), Vec2f::new(x1, y1)),
        (Vec2f::new(x1, y1), Vec2f::new(x0, y1)),
        (Vec2f::new(x0, y1), Vec2f::new(x0, y0)),
    ])
}

fn build_star_polygon(center: Vec2f, outer_radius: f32, inner_radius: f32, spikes: usize, angle: f32) -> Vec<Vec2f> {
    let spikes = spikes.max(2);
    let mut points = Vec::with_capacity(spikes * 2);
    for idx in 0..spikes * 2 {
        let radius = if idx % 2 == 0 { outer_radius } else { inner_radius };
        let theta = angle + idx as f32 * PI / spikes as f32;
        points.push(Vec2f::new(center.x + theta.cos() * radius, center.y + theta.sin() * radius));
    }
    points
}

fn build_square_polygon(center: Vec2f, radius: f32) -> [Vec2f; 4] {
    [
        Vec2f::new(center.x - radius, center.y - radius),
        Vec2f::new(center.x + radius, center.y - radius),
        Vec2f::new(center.x + radius, center.y + radius),
        Vec2f::new(center.x - radius, center.y + radius),
    ]
}

fn build_diamond_polygon(center: Vec2f, radius: f32) -> [Vec2f; 4] {
    [
        Vec2f::new(center.x, center.y - radius),
        Vec2f::new(center.x + radius, center.y),
        Vec2f::new(center.x, center.y + radius),
        Vec2f::new(center.x - radius, center.y),
    ]
}

/// Creates the documented oblique orbit camera and finite bounds for the fullscreen grid.
fn create_grid_view_3d() -> View3D {
    // Camera::new positions cameras by rotating the +Z basis vector. This shortest-arc
    // quaternion maps that basis onto the original `(24, -32, 26)` viewing offset, preserving the
    // useful initial composition while allowing View3D to accumulate arbitrary arcball rotation.
    let camera_offset = Vec3f::new(24.0, -32.0, 26.0);
    let camera_direction = Vec3f::normalize(&camera_offset);
    let basis = Vec3f::new(0.0, 0.0, 1.0);
    let axis = Vec3f::cross(&basis, &camera_direction);
    let rotation = Quatf::normalize(&Quatf::new(axis.x, axis.y, axis.z, 1.0 + Vec3f::dot(&basis, &camera_direction)));
    let extent = GRID_HALF_EXTENT as f32;
    let bounds = Box3f::new(&Vec3f::new(-extent, -extent, -0.1), &Vec3f::new(extent, extent, 0.1));
    let camera = Camera::new(
        Vec3f::new(0.0, 0.0, 0.0),
        camera_offset.length(),
        rotation,
        std::f32::consts::FRAC_PI_3,
        1.0,
        0.1,
        GRID_MAX_CAMERA_DISTANCE + extent * 2.0,
    );
    View3D::new(camera, Dimensioni::new(1, 1), bounds)
}

/// Projection state reused while the fullscreen callback converts world-space grid lines.
struct GridProjection {
    /// Interactive projection-view-model matrix copied from the shared [`View3D`].
    pvm: Mat4f,
    /// Current custom-render allocation used to map normalized device coordinates to pixels.
    area: Recti,
    /// Atlas coordinate sampled by every solid colored line triangle.
    white_uv: Vec2f,
}

impl GridProjection {
    /// Captures one render allocation and its already aspect-correct interactive projection.
    fn new(area: Recti, white_uv: Vec2f, pvm: Mat4f) -> Self {
        Self { pvm, area, white_uv }
    }

    /// Clips a world-space segment against all six homogeneous view-frustum planes.
    fn clip_segment(&self, start: Vec3f, end: Vec3f) -> Option<(Vec4f, Vec4f)> {
        let start = self.pvm * Vec4f::new(start.x, start.y, start.z, 1.0);
        let end = self.pvm * Vec4f::new(end.x, end.y, end.z, 1.0);
        if !clip_point_is_finite(start) || !clip_point_is_finite(end) {
            return None;
        }

        // A point lies inside the OpenGL-style homogeneous frustum when every expression below
        // is non-negative. Parametric clipping happens before division by W, preventing segments
        // behind the camera from producing the enormous stray screen coordinates seen previously.
        let start_planes = clip_plane_distances(start);
        let end_planes = clip_plane_distances(end);
        let mut enter = 0.0_f32;
        let mut leave = 1.0_f32;
        for (start_distance, end_distance) in start_planes.into_iter().zip(end_planes) {
            if start_distance < 0.0 && end_distance < 0.0 {
                return None;
            }
            if start_distance < 0.0 || end_distance < 0.0 {
                let denominator = start_distance - end_distance;
                if denominator.abs() <= f32::EPSILON {
                    return None;
                }
                let intersection = start_distance / denominator;
                if start_distance < 0.0 {
                    enter = enter.max(intersection);
                } else {
                    leave = leave.min(intersection);
                }
                if enter > leave {
                    return None;
                }
            }
        }

        let delta = end - start;
        let clipped_start = start + delta * enter;
        let clipped_end = start + delta * leave;
        (clipped_start.w > f32::EPSILON && clipped_end.w > f32::EPSILON).then_some((clipped_start, clipped_end))
    }

    /// Converts one clipped homogeneous point to microui's top-left-origin screen coordinates.
    fn screen_point(&self, point: Vec4f) -> Option<Vec2f> {
        if !clip_point_is_finite(point) || point.w <= f32::EPSILON {
            return None;
        }
        let normalized_x = point.x / point.w;
        let normalized_y = point.y / point.w;
        let screen = Vec2f::new(
            self.area.x as f32 + (normalized_x * 0.5 + 0.5) * self.area.width as f32,
            self.area.y as f32 + (-normalized_y * 0.5 + 0.5) * self.area.height as f32,
        );
        (screen.x.is_finite() && screen.y.is_finite()).then_some(screen)
    }

    /// Projects a 3D segment and appends a constant-pixel-width screen-space quad.
    fn push_line(&self, vertices: &mut Vec<Vertex>, start: Vec3f, end: Vec3f, width: f32, color: Color4b) {
        // The world endpoints establish perspective geometry. Clip them before the homogeneous
        // divide, then apply thickness after projection so distant lines remain legible instead of
        // collapsing below one framebuffer pixel as a thin world-space mesh would.
        let Some((start, end)) = self.clip_segment(start, end) else {
            return;
        };
        let Some(start) = self.screen_point(start) else {
            return;
        };
        let Some(end) = self.screen_point(end) else {
            return;
        };
        let delta = end - start;
        let length = delta.length();
        if !length.is_finite() || length <= f32::EPSILON {
            return;
        }

        // Rotate the normalized segment direction ninety degrees to obtain the two quad edges.
        let half_width = width.max(0.5) * 0.5;
        let normal = Vec2f::new(-delta.y / length, delta.x / length) * half_width;
        let a = start + normal;
        let b = end + normal;
        let c = end - normal;
        let d = start - normal;
        for point in [a, b, c, a, c, d] {
            vertices.push(Vertex::new(point, self.white_uv, color));
        }
    }
}

/// Returns whether every component is safe for homogeneous clipping and division.
fn clip_point_is_finite(point: Vec4f) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.z.is_finite() && point.w.is_finite()
}

/// Evaluates the six canonical homogeneous frustum half-spaces for one clip-space point.
fn clip_plane_distances(point: Vec4f) -> [f32; 6] {
    [
        point.x + point.w,
        point.w - point.x,
        point.y + point.w,
        point.w - point.y,
        point.z + point.w,
        point.w - point.z,
    ]
}

/// Builds a perspective X-Y ground grid for the dedicated fullscreen background window.
fn build_xy_grid_vertices(area: Recti, white_uv: Vec2f, show_minor_lines: bool, pvm: Mat4f) -> Vec<Vertex> {
    if area.width <= 0 || area.height <= 0 {
        return Vec::new();
    }

    let projection = GridProjection::new(area, white_uv, pvm);
    let mut vertices = Vec::with_capacity((GRID_HALF_EXTENT as usize * 4 + 2) * 6);

    // Draw non-axis lines first so the red X and green Y axes remain visually authoritative at the
    // origin. Major divisions survive when the menu disables the denser unit-spaced lines.
    for coordinate in -GRID_HALF_EXTENT..=GRID_HALF_EXTENT {
        if coordinate == 0 {
            continue;
        }
        let major = coordinate % GRID_MAJOR_INTERVAL == 0;
        if !major && !show_minor_lines {
            continue;
        }
        let coordinate = coordinate as f32;
        let extent = GRID_HALF_EXTENT as f32;
        let (width, line_color) = if major {
            (1.5, color4b(78, 92, 118, 230))
        } else {
            (1.0, color4b(48, 58, 76, 180))
        };
        projection.push_line(
            &mut vertices,
            Vec3f::new(coordinate, -extent, 0.0),
            Vec3f::new(coordinate, extent, 0.0),
            width,
            line_color,
        );
        projection.push_line(
            &mut vertices,
            Vec3f::new(-extent, coordinate, 0.0),
            Vec3f::new(extent, coordinate, 0.0),
            width,
            line_color,
        );
    }

    // Conventional axis colors distinguish the two directions without requiring text labels in
    // the non-interactive rendering surface.
    let extent = GRID_HALF_EXTENT as f32;
    projection.push_line(
        &mut vertices,
        Vec3f::new(-extent, 0.0, 0.0),
        Vec3f::new(extent, 0.0, 0.0),
        2.5,
        color4b(220, 72, 72, 255),
    );
    projection.push_line(
        &mut vertices,
        Vec3f::new(0.0, -extent, 0.0),
        Vec3f::new(0.0, extent, 0.0),
        2.5,
        color4b(72, 205, 112, 255),
    );
    vertices
}

fn build_triangle_vertices(area: Recti, white_uv: Vec2f, angle: f32) -> Vec<Vertex> {
    let (sin_theta, cos_theta) = angle.sin_cos();
    let half_w = (area.width.max(1) as f32) * 0.5;
    let half_h = (area.height.max(1) as f32) * 0.5;
    let cx = area.x as f32 + half_w;
    let cy = area.y as f32 + half_h;

    let order = [0usize, 2, 1]; // convert to clockwise winding for Vulkan UI pipeline
    order
        .iter()
        .map(|tv| {
            let tv = &TRI_VERTS[*tv];
            let rx = tv.pos.x * cos_theta - tv.pos.y * sin_theta;
            let ry = tv.pos.x * sin_theta + tv.pos.y * cos_theta;
            let pos = Vec2f::new(cx + rx * half_w, cy + ry * half_h);
            Vertex::new(pos, white_uv, tv.color)
        })
        .collect()
}

fn build_mesh_buffers(mesh: &PolyMesh) -> MeshBuffers {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for poly in mesh.polys() {
        let start = vertices.len() as u32;
        let mut count = 0;
        for v in poly {
            let position = mesh.vertex_position(v.pos);
            let normal = mesh.vertex_normal(v.normal);
            let uv = mesh.vertex_uv(v.tex);
            vertices.push(MeshVertex {
                position: [position.x, position.y, position.z],
                normal: [normal.x, normal.y, normal.z],
                uv: [uv.x, uv.y],
            });
            count += 1;
        }
        for i in 2..count {
            indices.push(start);
            indices.push(start + i as u32 - 1);
            indices.push(start + i as u32);
        }
    }
    MeshBuffers::from_vecs(vertices, indices)
}

fn demo_asset_path(relative: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[cfg(any(feature = "builder", feature = "png_source"))]
fn load_external_image_texture(ctx: &mut Context<SelectedBackend, State>) -> Option<TextureId> {
    let image_path = demo_asset_path("examples/FACEPALM.png");
    let png_bytes = match fs::read(&image_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("Failed to read {}: {err}", image_path.display());
            return None;
        }
    };
    match ctx.load_image_from(ImageSource::Png { bytes: png_bytes.as_slice() }) {
        Ok(texture) => Some(texture),
        Err(err) => {
            eprintln!("Failed to decode {}: {err}", image_path.display());
            None
        }
    }
}

#[cfg(test)]
mod grid_tests {
    use super::*;

    /// Produces the same aspect-correct initial PVM used by the live fullscreen widget.
    fn initial_grid_pvm(area: Recti) -> Mat4f {
        let mut view = create_grid_view_3d();
        view.set_dimension(Dimensioni::new(area.width.max(1), area.height.max(1)));
        view.pvm()
    }

    #[test]
    fn perspective_grid_density_changes_only_minor_line_geometry() {
        let area = rect(0, 20, 1024, 748);
        let white_uv = Vec2f::new(0.5, 0.5);
        let pvm = initial_grid_pvm(area);
        let dense = build_xy_grid_vertices(area, white_uv, true, pvm);
        let major_only = build_xy_grid_vertices(area, white_uv, false, pvm);

        // Frustum clipping may discard whole lines at narrow aspect ratios, so assert structural
        // geometry invariants instead of coupling the test to one exact camera composition.
        assert!(!major_only.is_empty());
        assert!(dense.len() > major_only.len());
        assert_eq!(dense.len() % 6, 0);
        assert_eq!(major_only.len() % 6, 0);
        assert!(dense.iter().all(|vertex| {
            let position = vertex.position();
            position.x.is_finite()
                && position.y.is_finite()
                && position.x >= area.x as f32 - 2.0
                && position.x <= (area.x + area.width) as f32 + 2.0
                && position.y >= area.y as f32 - 2.0
                && position.y <= (area.y + area.height) as f32 + 2.0
        }));
    }

    #[test]
    fn empty_grid_viewport_emits_no_custom_geometry() {
        assert!(build_xy_grid_vertices(rect(0, 0, 0, 600), Vec2f::default(), true, Mat4f::identity()).is_empty());
        assert!(build_xy_grid_vertices(rect(0, 0, 800, 0), Vec2f::default(), true, Mat4f::identity()).is_empty());
    }

    #[test]
    fn homogeneous_clipping_bounds_a_segment_that_crosses_the_viewport() {
        let area = rect(10, 20, 800, 600);
        let projection = GridProjection::new(area, Vec2f::default(), Mat4f::identity());
        let mut vertices = Vec::new();

        projection.push_line(
            &mut vertices,
            Vec3f::new(-2.0, 0.0, 0.0),
            Vec3f::new(2.0, 0.0, 0.0),
            2.0,
            color4b(255, 255, 255, 255),
        );

        assert_eq!(vertices.len(), 6);
        assert!(vertices.iter().all(|vertex| {
            let position = vertex.position();
            position.x >= area.x as f32 - 1.1
                && position.x <= (area.x + area.width) as f32 + 1.1
                && position.y >= area.y as f32 - 1.1
                && position.y <= (area.y + area.height) as f32 + 1.1
        }));
    }

    #[test]
    fn arcball_and_zoom_change_the_grid_projection_without_invalid_vertices() {
        let area = rect(0, 0, 800, 600);
        let mut view = create_grid_view_3d();
        view.set_dimension(Dimensioni::new(area.width, area.height));
        let before = build_xy_grid_vertices(area, Vec2f::default(), false, view.pvm());

        let _ = view.update_drag(Vec2i::new(400, 300), Vec2i::new(460, 330));
        let _ = view.apply_scroll_with_limits(-3.0, GRID_MIN_CAMERA_DISTANCE, GRID_MAX_CAMERA_DISTANCE);
        let after = build_xy_grid_vertices(area, Vec2f::default(), false, view.pvm());

        assert!(!before.is_empty());
        assert!(!after.is_empty());
        assert!(
            before
                .iter()
                .zip(after.iter())
                .any(|(left, right)| (left.position() - right.position()).length() > 0.01)
        );
        assert!(after.iter().all(|vertex| {
            let position = vertex.position();
            position.x.is_finite() && position.y.is_finite()
        }));
    }
}
