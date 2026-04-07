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
#[path = "./common/mod.rs"]
mod common;

use common::{application::Application, application::BackendInitContext, atlas_assets, camera::Camera, obj_loader::Obj, polymesh::PolyMesh, view3d::View3D};
#[cfg(feature = "example-glow")]
use common::glow_renderer::{CustomRenderArea, GLRenderer as BackendRenderer, MeshBuffers, MeshSubmission, MeshVertex};
#[cfg(all(not(feature = "example-glow"), feature = "example-vulkan"))]
use common::vulkan_renderer::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex, VulkanRenderer as BackendRenderer};
#[cfg(all(not(feature = "example-glow"), not(feature = "example-vulkan"), feature = "example-wgpu"))]
use common::wgpu_renderer::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex, WgpuRenderer as BackendRenderer};
#[cfg(feature = "builder")]
use microui_redux::builder;
use microui_redux::*;
use rand::{RngExt, rng};
use std::{
    cell::RefCell,
    f32::consts::PI,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, RwLock},
    time::Instant,
};

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

#[derive(Clone)]
struct GraphicsDemo {
    phase: f32,
    opt: WidgetOption,
    bopt: WidgetBehaviourOption,
}

impl GraphicsDemo {
    fn new() -> Self {
        Self {
            phase: 0.0,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
        }
    }
}

impl Widget for GraphicsDemo {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn behaviour_opt(&self) -> &WidgetBehaviourOption {
        &self.bopt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        Dimensioni::new(240, 200)
    }

    fn needs_input_snapshot(&self) -> bool {
        true
    }

    fn run(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let bounds = ctx.rect();
        let local_width = bounds.width.max(0) as f32;
        let local_height = bounds.height.max(0) as f32;
        if local_width <= 0.0 || local_height <= 0.0 {
            return ResourceState::NONE;
        }

        self.phase = (self.phase + 0.025) % (PI * 2.0);
        let clip_rect = rect(18, 18, (bounds.width - 36).max(0), (bounds.height - 36).max(0));
        let animated_center = Vec2f::new(
            local_width * 0.5 + self.phase.cos() * (local_width * 0.16),
            local_height * 0.5 + self.phase.sin() * (local_height * 0.12),
        );
        let star_center = if control.hovered {
            ctx.input()
                .map(|input| Vec2f::new(input.mouse_pos.x as f32, input.mouse_pos.y as f32))
                .unwrap_or(animated_center)
        } else {
            animated_center
        };
        let star_center = Vec2f::new(star_center.x.clamp(0.0, local_width), star_center.y.clamp(0.0, local_height));

        ctx.graphics(|g| {
            let local = g.local_rect();
            let outer = rect(8, 8, (local.width - 16).max(0), (local.height - 16).max(0));
            let background = [
                Vec2f::new(0.0, 0.0),
                Vec2f::new(local_width, 0.0),
                Vec2f::new(local_width, local_height),
                Vec2f::new(0.0, local_height),
            ];

            g.fill_polygon(background.as_slice(), color(34, 38, 44, 255));
            stroke_graphics_rect(g, outer, 2.0, color(65, 70, 76, 255));
            stroke_graphics_rect(g, clip_rect, 1.5, color(240, 210, 110, 255));

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
        });

        ResourceState::NONE
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

#[derive(Clone)]
struct FalloffEditor {
    nodes: Vec<FalloffNode>,
    active: Option<FalloffTarget>,
    hovered: Option<FalloffTarget>,
    opt: WidgetOption,
    bopt: WidgetBehaviourOption,
}

impl FalloffEditor {
    fn new() -> Self {
        let mut editor = Self {
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
            opt: WidgetOption::HOLD_FOCUS,
            bopt: WidgetBehaviourOption::NONE,
        };
        editor.sanitize();
        editor
    }

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
                let dx = pos.x - mouse_local.x;
                let dy = pos.y - mouse_local.y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= best_dist_sq {
                    best = Some(target);
                    best_dist_sq = dist_sq;
                }
            }

            if idx > 0 {
                let target = FalloffTarget::InHandle(idx);
                let pos = self.target_local(graph, target);
                let dx = pos.x - mouse_local.x;
                let dy = pos.y - mouse_local.y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= best_dist_sq {
                    best = Some(target);
                    best_dist_sq = dist_sq;
                }
            }

            if idx + 1 < self.nodes.len() {
                let target = FalloffTarget::OutHandle(idx);
                let pos = self.target_local(graph, target);
                let dx = pos.x - mouse_local.x;
                let dy = pos.y - mouse_local.y;
                let dist_sq = dx * dx + dy * dy;
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

    fn behaviour_opt(&self) -> &WidgetBehaviourOption {
        &self.bopt
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        Dimensioni::new(300, 220)
    }

    fn needs_input_snapshot(&self) -> bool {
        true
    }

    fn run(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let bounds = ctx.rect();
        let graph = Self::graph_rect(bounds);
        if graph.width <= 0 || graph.height <= 0 {
            return ResourceState::NONE;
        }

        let mut changed = false;
        let input = ctx.input();

        if !control.focused && !control.active {
            self.active = None;
        }

        if let Some(input) = input {
            let mouse_local = Vec2f::new(input.mouse_pos.x as f32, input.mouse_pos.y as f32);
            self.hovered = if control.hovered { self.pick_target(graph, mouse_local) } else { None };

            if control.clicked {
                self.active = if graph.contains(&Vec2i::new(mouse_local.x as i32, mouse_local.y as i32)) {
                    self.pick_target(graph, mouse_local)
                } else {
                    None
                };
            } else if !control.active {
                self.active = None;
            }

            if control.active && (input.mouse_delta.x != 0 || input.mouse_delta.y != 0) {
                if let Some(target) = self.active {
                    let point = Self::local_to_graph(graph, mouse_local);
                    self.drag_target(target, point);
                    changed = true;
                }
            }
        } else {
            self.hovered = None;
            self.active = None;
        }

        let curve = self.sample_curve_local(graph, FALLOFF_SEGMENT_STEPS);

        ctx.graphics(|g| {
            let local = g.local_rect();
            let background = [
                Vec2f::new(0.0, 0.0),
                Vec2f::new(local.width as f32, 0.0),
                Vec2f::new(local.width as f32, local.height as f32),
                Vec2f::new(0.0, local.height as f32),
            ];
            g.fill_polygon(background.as_slice(), color(25, 29, 34, 255));
            stroke_graphics_rect(
                g,
                rect(6, 6, (local.width - 12).max(0), (local.height - 12).max(0)),
                1.5,
                color(62, 68, 76, 255),
            );
            stroke_graphics_rect(g, graph, 1.5, color(88, 96, 106, 255));

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
        });

        if self.active.is_some() {
            let mut state = ResourceState::ACTIVE;
            if changed {
                state |= ResourceState::CHANGE;
            }
            state
        } else if changed {
            ResourceState::CHANGE
        } else {
            ResourceState::NONE
        }
    }
}

struct SuzaneData {
    view_3d: View3D,
    mesh: MeshBuffers,
}

fn static_label(text: impl Into<String>) -> WidgetHandle<ListItem> {
    widget_handle(ListItem::with_opt(text, WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME))
}

struct State {
    renderer: RendererHandle<BackendRenderer>,
    bg: [Real; 3],
    bg_sliders: [WidgetHandle<Slider>; 3],
    style_color_sliders: [WidgetHandle<Slider>; 60],
    style_value_sliders: [WidgetHandle<Slider>; 5],
    logbuf: Rc<RefCell<String>>,
    logbuf_updated: bool,
    submit_buf: WidgetHandle<Textbox>,
    text_area: WidgetHandle<TextArea>,
    combo_state: WidgetHandle<Combo>,
    combo_items: [WidgetHandle<ListItem>; 4],
    style_color_labels: [WidgetHandle<ListItem>; 14],
    style_color_swatches: [WidgetHandle<ColorSwatch>; 14],
    style_metric_labels: [WidgetHandle<ListItem>; 5],
    stack_direction_labels: [WidgetHandle<ListItem>; 2],
    weight_labels: [WidgetHandle<ListItem>; 2],
    window_info_labels: [WidgetHandle<ListItem>; 3],
    window_info_values: [WidgetHandle<ListItem>; 3],
    test_button_labels: [WidgetHandle<ListItem>; 3],
    tree_labels: [WidgetHandle<ListItem>; 2],
    background_labels: [WidgetHandle<ListItem>; 3],
    style: Style,

    demo_window: Option<WindowHandle>,
    style_window: Option<WindowHandle>,
    log_window: Option<WindowHandle>,
    popup_window: Option<WindowHandle>,
    log_output: Option<ContainerHandle>,
    typography_window: Option<WindowHandle>,
    triangle_window: Option<WindowHandle>,
    graphics_window: Option<WindowHandle>,
    falloff_window: Option<WindowHandle>,
    suzane_window: Option<WindowHandle>,
    stack_direction_window: Option<WindowHandle>,
    weight_window: Option<WindowHandle>,
    dialog_window: Option<FileDialogState>,

    fps: f32,
    last_frame: Instant,

    window_header: WidgetHandle<Node>,
    test_buttons_header: WidgetHandle<Node>,
    background_header: WidgetHandle<Node>,
    tree_and_text_header: WidgetHandle<Node>,
    text_area_header: WidgetHandle<Node>,
    slot_header: WidgetHandle<Node>,
    combo_header: WidgetHandle<Node>,
    test1_tn: WidgetHandle<Node>,
    test1a_tn: WidgetHandle<Node>,
    test1b_tn: WidgetHandle<Node>,
    test2_tn: WidgetHandle<Node>,
    test3_tn: WidgetHandle<Node>,
    submit_button: WidgetHandle<Button>,
    log_text: WidgetHandle<TextBlock>,
    typography_heading: WidgetHandle<TextBlock>,
    typography_body: WidgetHandle<TextBlock>,
    typography_button: WidgetHandle<Button>,
    test_buttons: [WidgetHandle<Button>; 6],
    tree_buttons: [WidgetHandle<Button>; 6],
    popup_buttons: [WidgetHandle<Button>; 2],
    slot_buttons: [WidgetHandle<Button>; 4],
    stack_direction_buttons: [WidgetHandle<Button>; 6],
    weight_buttons: [WidgetHandle<Button>; 9],
    external_image_button: Option<WidgetHandle<Button>>,
    checkboxes: [WidgetHandle<Checkbox>; 3],
    open_popup: bool,
    open_dialog: bool,
    white_uv: Vec2f,
    triangle_data: Arc<RwLock<TriangleState>>,
    suzane_data: Arc<RwLock<SuzaneData>>,
    triangle_widget: WidgetHandle<Custom>,
    graphics_widget: WidgetHandle<GraphicsDemo>,
    falloff_widget: WidgetHandle<FalloffEditor>,
    suzane_widget: WidgetHandle<Custom>,
    background_swatch: WidgetHandle<ColorSwatch>,
    style_tree: WidgetTree,
    log_tree: WidgetTree,
    typography_tree: WidgetTree,
    triangle_tree: WidgetTree,
    graphics_tree: WidgetTree,
    falloff_tree: WidgetTree,
    suzane_tree: WidgetTree,
    stack_direction_tree: WidgetTree,
    weight_tree: WidgetTree,
    demo_tree: WidgetTree,
    combo_tree: WidgetTree,
    popup_tree: WidgetTree,
}

impl State {
    pub fn new(_backend: BackendInitContext, renderer: RendererHandle<BackendRenderer>, slots: Vec<SlotId>, ctx: &mut Context<BackendRenderer>) -> Self {
        #[cfg(any(feature = "builder", feature = "png_source"))]
        let image_texture = load_external_image_texture(ctx);
        #[cfg(not(any(feature = "builder", feature = "png_source")))]
        let image_texture = None;
        let white_uv = renderer.scope(|r| {
            let atlas = r.get_atlas();
            let rect = atlas.get_icon_rect(WHITE_ICON);
            let dim = atlas.get_texture_dimension();
            Vec2f::new(
                (rect.x as f32 + rect.width as f32 * 0.5) / dim.width as f32,
                (rect.y as f32 + rect.height as f32 * 0.5) / dim.height as f32,
            )
        });

        let triangle_data = Arc::new(RwLock::new(TriangleState { angle: 0.0 }));
        let suzane_path = demo_asset_path("assets/suzane.obj");
        let suzane_bytes = fs::read(&suzane_path).unwrap_or_else(|err| panic!("Failed to read {}: {err}", suzane_path.display()));
        let pm_suzane = Obj::from_byte_stream(suzane_bytes.as_slice())
            .unwrap_or_else(|err| panic!("Failed to parse {}: {err}", suzane_path.display()))
            .to_polymesh();
        let bounds = pm_suzane.calculate_bounding_box();
        let mesh_buffers = build_mesh_buffers(&pm_suzane);
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
        let suzane_data = Arc::new(RwLock::new(SuzaneData { view_3d, mesh: mesh_buffers }));

        let rng = Rc::new(RefCell::new(rng()));
        let green_paint: Rc<dyn Fn(usize, usize) -> Color4b> = Rc::new(|_x, _y| color4b(0x00, 0xFF, 0x00, 0xFF));
        let random_paint: Rc<dyn Fn(usize, usize) -> Color4b> = {
            let rng = rng.clone();
            Rc::new(move |_x, _y| {
                let mut rm = rng.borrow_mut();
                color4b(rm.random(), rm.random(), rm.random(), rm.random())
            })
        };
        let slot_buttons = [
            widget_handle(Button::with_image(
                "Slot 1",
                Some(Image::Slot(slots[0])),
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
            widget_handle(Button::with_slot(
                "Slot 2 - Green",
                slots[1],
                green_paint,
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
            widget_handle(Button::with_image(
                "Slot 3",
                Some(Image::Slot(slots[2])),
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
            widget_handle(Button::with_slot(
                "Slot 2 - Random",
                slots[1],
                random_paint,
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
        ];
        let external_image_button = image_texture.map(|texture| {
            widget_handle(Button::with_image(
                "External Image",
                Some(Image::Texture(texture)),
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            ))
        });
        let style_color_sliders = std::array::from_fn(|_| widget_handle(Slider::with_opt(0.0, 0.0, 255.0, 0.0, 0, WidgetOption::ALIGN_CENTER)));
        let style_color_swatches = std::array::from_fn(|_| widget_handle(ColorSwatch::new(color(0, 0, 0, 0xFF))));
        let style_value_sliders = [
            widget_handle(Slider::with_opt(0.0, 0.0, 16.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 16.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
        ];
        let bg_sliders = std::array::from_fn(|_| widget_handle(Slider::with_opt(0.0, 0.0, 255.0, 0.0, 0, WidgetOption::ALIGN_CENTER)));
        let mut text_area =
            TextArea::new("This is a multi-line TextArea.\nYou can type, scroll, and resize the window.\n\nTry adding more lines to see the scrollbars.");
        text_area.wrap = TextWrap::Word;
        let mut submit_buf = Textbox::new("");
        submit_buf.font = FontRole::Mono.into();
        let mut log_text = TextBlock::new("");
        log_text.font = FontRole::Mono.into();
        let mut typography_heading = TextBlock::new("NORMAL.ttf at 18px");
        typography_heading.font = FontRole::Heading.into();
        let mut typography_body = TextBlock::with_wrap(
            "NORMAL.ttf at 12px remains the control font. Window titles use BOLD.ttf, and the log window uses CONSOLE.ttf for input and output.",
            TextWrap::Word,
        );
        typography_body.font = FontRole::Body.into();
        let style = Style::default().with_named_fonts(&ctx.canvas().get_atlas());
        let mut state = Self {
            renderer,
            bg: [90.0, 95.0, 100.0],
            bg_sliders,
            style_color_sliders,
            style_value_sliders,
            logbuf: Rc::new(RefCell::new(String::new())),
            logbuf_updated: false,
            submit_buf: widget_handle(submit_buf),
            text_area: widget_handle(text_area),
            combo_state: widget_handle(Combo::new(ctx.new_popup("Combo Box Popup"))),
            combo_items: [
                widget_handle(ListItem::new("Apple")),
                widget_handle(ListItem::new("Banana")),
                widget_handle(ListItem::new("Cherry")),
                widget_handle(ListItem::new("Date")),
            ],
            style_color_labels: [
                static_label("text"),
                static_label("border:"),
                static_label("windowbg:"),
                static_label("titlebg:"),
                static_label("titletext:"),
                static_label("panelbg:"),
                static_label("button:"),
                static_label("buttonhover:"),
                static_label("buttonfocus:"),
                static_label("base:"),
                static_label("basehover:"),
                static_label("basefocus:"),
                static_label("scrollbase:"),
                static_label("scrollthumb:"),
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
            window_info_values: [static_label(""), static_label(""), static_label("")],
            test_button_labels: [
                static_label("Test buttons 1:"),
                static_label("Test buttons 2:"),
                static_label("Test buttons 3:"),
            ],
            tree_labels: [static_label("Hello"), static_label("world")],
            background_labels: [static_label("Red:"), static_label("Green:"), static_label("Blue:")],
            style,
            demo_window: Some(ctx.new_window("Demo Window", rect(40, 40, 300, 450))),
            style_window: Some(ctx.new_window("Style Editor", rect(350, 250, 300, 240))),
            log_window: Some(ctx.new_window("Log Window", rect(350, 40, 300, 200))),
            popup_window: Some(ctx.new_popup("Test Popup")),
            log_output: Some(ctx.new_panel("Log Output")),
            typography_window: Some(ctx.new_window("Typography Demo", rect(40, 500, 300, 170))),
            triangle_window: Some(ctx.new_window("Triangle Window", rect(200, 100, 200, 200))),
            graphics_window: Some(ctx.new_window("Graphics Window", rect(820, 40, 280, 240))),
            falloff_window: Some(ctx.new_window("Brush Falloff", rect(820, 300, 320, 260))),
            suzane_window: Some(ctx.new_window("Suzane Window", rect(220, 220, 300, 300))),
            stack_direction_window: Some(ctx.new_window("Stack Direction Demo", rect(530, 40, 280, 220))),
            weight_window: Some(ctx.new_window("Weight Demo", rect(530, 270, 280, 260))),
            dialog_window: Some(FileDialogState::new(ctx)),
            fps: 0.0,
            last_frame: Instant::now(),
            window_header: widget_handle(Node::header("Window Info", NodeStateValue::Closed)),
            test_buttons_header: widget_handle(Node::header("Test Buttons", NodeStateValue::Expanded)),
            background_header: widget_handle(Node::header("Background Color", NodeStateValue::Expanded)),
            tree_and_text_header: widget_handle(Node::header("Tree and Text", NodeStateValue::Expanded)),
            text_area_header: widget_handle(Node::header("TextArea", NodeStateValue::Expanded)),
            slot_header: widget_handle(Node::header("Slots", NodeStateValue::Expanded)),
            combo_header: widget_handle(Node::header("Combo Box", NodeStateValue::Expanded)),
            test1_tn: widget_handle(Node::tree("Test 1", NodeStateValue::Closed)),
            test1a_tn: widget_handle(Node::tree("Test 1a", NodeStateValue::Closed)),
            test1b_tn: widget_handle(Node::tree("Test 1b", NodeStateValue::Closed)),
            test2_tn: widget_handle(Node::tree("Test 2", NodeStateValue::Closed)),
            test3_tn: widget_handle(Node::tree("Test 3", NodeStateValue::Closed)),
            submit_button: widget_handle(Button::with_opt("Submit", WidgetOption::ALIGN_CENTER)),
            log_text: widget_handle(log_text),
            typography_heading: widget_handle(typography_heading),
            typography_body: widget_handle(typography_body),
            typography_button: widget_handle(Button::with_opt("Control Preview", WidgetOption::ALIGN_CENTER)),
            test_buttons: [
                widget_handle(Button::with_opt("Button 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Popup", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 4", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Dialog", WidgetOption::ALIGN_CENTER)),
            ],
            tree_buttons: [
                widget_handle(Button::with_opt("Button 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 4", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 5", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 6", WidgetOption::ALIGN_CENTER)),
            ],
            popup_buttons: [
                widget_handle(Button::with_opt("Hello", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("World", WidgetOption::ALIGN_CENTER)),
            ],
            slot_buttons,
            stack_direction_buttons: [
                widget_handle(Button::with_opt("Call 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 3", WidgetOption::ALIGN_CENTER)),
            ],
            weight_buttons: [
                widget_handle(Button::with_opt("w1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("w2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("w3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g4", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g5", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g6", WidgetOption::ALIGN_CENTER)),
            ],
            external_image_button,
            checkboxes: [
                widget_handle(Checkbox::new("Checkbox 1", false)),
                widget_handle(Checkbox::new("Checkbox 2", true)),
                widget_handle(Checkbox::new("Checkbox 3", false)),
            ],
            open_popup: false,
            open_dialog: false,
            white_uv,
            triangle_data,
            suzane_data,
            triangle_widget: widget_handle(Custom::with_opt("Triangle", WidgetOption::HOLD_FOCUS, WidgetBehaviourOption::NONE)),
            graphics_widget: widget_handle(GraphicsDemo::new()),
            falloff_widget: widget_handle(FalloffEditor::new()),
            suzane_widget: widget_handle(Custom::with_opt("Suzane", WidgetOption::HOLD_FOCUS, WidgetBehaviourOption::GRAB_SCROLL)),
            background_swatch: widget_handle(ColorSwatch::new(color(90, 95, 100, 0xFF))),
            style_tree: WidgetTree::default(),
            log_tree: WidgetTree::default(),
            typography_tree: WidgetTree::default(),
            triangle_tree: WidgetTree::default(),
            graphics_tree: WidgetTree::default(),
            falloff_tree: WidgetTree::default(),
            suzane_tree: WidgetTree::default(),
            stack_direction_tree: WidgetTree::default(),
            weight_tree: WidgetTree::default(),
            demo_tree: WidgetTree::default(),
            combo_tree: WidgetTree::default(),
            popup_tree: WidgetTree::default(),
        };
        state.rebuild_trees();
        state
    }

    fn write_log(&mut self, text: &str) {
        let mut logbuf = self.logbuf.borrow_mut();
        if !logbuf.is_empty() {
            logbuf.push('\n');
        }
        for c in text.chars() {
            logbuf.push(c);
        }
        self.logbuf_updated = true;
    }

    fn section(tree: &mut WidgetTreeBuilder, node: &WidgetHandle<Node>, f: impl FnOnce(&mut WidgetTreeBuilder)) {
        tree.header(node.clone(), f);
    }

    fn rebuild_trees(&mut self) {
        let style_color_labels = self.style_color_labels.clone();
        let style_color_sliders = self.style_color_sliders.clone();
        let style_color_swatches = self.style_color_swatches.clone();
        let style_metric_labels = self.style_metric_labels.clone();
        let style_value_sliders = self.style_value_sliders.clone();
        self.style_tree = WidgetTreeBuilder::build(move |tree| {
            let color_row = [
                SizePolicy::Fixed(80),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
            ];
            let metrics_row = [SizePolicy::Fixed(80), SizePolicy::Remainder(0)];

            for ((label, sliders), swatch) in style_color_labels
                .iter()
                .zip(style_color_sliders.chunks_exact(4))
                .zip(style_color_swatches.iter())
            {
                tree.row(&color_row, SizePolicy::Auto, |tree| {
                    tree.widget(label.clone());
                    tree.widget(sliders[0].clone());
                    tree.widget(sliders[1].clone());
                    tree.widget(sliders[2].clone());
                    tree.widget(sliders[3].clone());
                    tree.widget(swatch.clone());
                });
            }

            for (label, slider) in style_metric_labels.iter().zip(style_value_sliders.iter()) {
                tree.row(&metrics_row, SizePolicy::Auto, |tree| {
                    tree.widget(label.clone());
                    tree.widget(slider.clone());
                });
            }
        });

        let log_output = self.log_output.clone().expect("log output panel missing");
        let log_text = self.log_text.clone();
        let submit_buf = self.submit_buf.clone();
        let submit_button = self.submit_button.clone();
        self.log_tree = WidgetTreeBuilder::build(move |tree| {
            let submit_row = [SizePolicy::Remainder(69), SizePolicy::Remainder(0)];
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(24), StackDirection::TopToBottom, |tree| {
                tree.container(log_output.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                    tree.widget(log_text.clone());
                });
            });
            tree.row(&submit_row, SizePolicy::Auto, |tree| {
                tree.widget(submit_buf.clone());
                tree.widget(submit_button.clone());
            });
        });

        let typography_heading = self.typography_heading.clone();
        let typography_body = self.typography_body.clone();
        let typography_button = self.typography_button.clone();
        self.typography_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                tree.widget(typography_heading.clone());
                tree.widget(typography_body.clone());
                tree.widget(typography_button.clone());
            });
        });

        let triangle_widget = self.triangle_widget.clone();
        let triangle_data = self.triangle_data.clone();
        let renderer = self.renderer.clone();
        let white_uv = self.white_uv;
        self.triangle_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                let triangle_data = triangle_data.clone();
                let renderer = renderer.clone();
                tree.custom_render(triangle_widget.clone(), move |_dim, cra| {
                    if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                        return;
                    }
                    let area = area_from_args(cra);
                    if let Ok(mut tri) = triangle_data.write() {
                        tri.angle = (tri.angle + 0.02) % (std::f32::consts::PI * 2.0);
                        let mut verts = build_triangle_vertices(area.rect, white_uv, tri.angle);
                        let mut renderer = renderer.clone();
                        renderer.scope_mut(move |vk| {
                            let verts_local = std::mem::take(&mut verts);
                            vk.enqueue_colored_vertices(area, verts_local);
                        });
                    }
                });
            });
        });

        let suzane_widget = self.suzane_widget.clone();
        let suzane_data = self.suzane_data.clone();
        let renderer = self.renderer.clone();
        self.suzane_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                let suzane_data = suzane_data.clone();
                let renderer = renderer.clone();
                tree.custom_render(suzane_widget.clone(), move |_dim, cra| {
                    if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                        return;
                    }
                    if let Ok(mut suzane) = suzane_data.write() {
                        suzane.view_3d.set_dimension(Dimensioni::new(cra.content_area.width, cra.content_area.height));
                        let _ = suzane.view_3d.update(cra.mouse_event);
                        if let Some(delta) = cra.scroll_delta {
                            let axis = if delta.y != 0 { delta.y } else { delta.x };
                            if axis != 0 {
                                suzane.view_3d.apply_scroll(axis as f32);
                            }
                        }
                        if !matches!(cra.mouse_event, MouseEvent::Drag { .. }) && cra.scroll_delta.is_none() {
                            let step = 20;
                            let mut delta = Vec2i::new(0, 0);
                            if cra.key_codes.is_left() {
                                delta.x -= step;
                            }
                            if cra.key_codes.is_right() {
                                delta.x += step;
                            }
                            if cra.key_codes.is_up() {
                                delta.y -= step;
                            }
                            if cra.key_codes.is_down() {
                                delta.y += step;
                            }
                            if delta.x != 0 || delta.y != 0 {
                                let center = Vec2i::new(cra.content_area.width / 2, cra.content_area.height / 2);
                                let curr = Vec2i::new(center.x + delta.x, center.y + delta.y);
                                suzane.view_3d.update(MouseEvent::Drag { prev_pos: center, curr_pos: curr });
                            }
                            for ch in cra.text_input.chars() {
                                match ch {
                                    'w' | 'W' => {
                                        let _ = suzane.view_3d.apply_scroll(-0.5);
                                    }
                                    's' | 'S' => {
                                        let _ = suzane.view_3d.apply_scroll(0.5);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let area = area_from_args(cra);
                        let submission = MeshSubmission {
                            mesh: suzane.mesh.clone(),
                            pvm: suzane.view_3d.pvm(),
                            view_model: suzane.view_3d.view_matrix(),
                        };
                        let mut renderer = renderer.clone();
                        renderer.scope_mut(|r| {
                            r.enqueue_mesh_draw(area, submission.clone());
                        });
                    }
                });
            });
        });

        let graphics_widget = self.graphics_widget.clone();
        self.graphics_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                tree.widget(graphics_widget.clone());
            });
        });

        let falloff_widget = self.falloff_widget.clone();
        self.falloff_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                tree.widget(falloff_widget.clone());
            });
        });

        let stack_direction_labels = self.stack_direction_labels.clone();
        let stack_direction_buttons = self.stack_direction_buttons.clone();
        self.stack_direction_tree = WidgetTreeBuilder::build(move |tree| {
            let columns = [SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)];
            let [label_top, label_bottom] = stack_direction_labels.clone();
            let [button_top_0, button_top_1, button_top_2, button_bottom_0, button_bottom_1, button_bottom_2] = stack_direction_buttons.clone();
            tree.row(&columns, SizePolicy::Auto, |tree| {
                tree.widget(label_top.clone());
                tree.widget(label_bottom.clone());
            });
            tree.row(&columns, SizePolicy::Fixed(120), |tree| {
                tree.column(|tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(28), StackDirection::TopToBottom, |tree| {
                        tree.widget(button_top_0.clone());
                        tree.widget(button_top_1.clone());
                        tree.widget(button_top_2.clone());
                    });
                });
                tree.column(|tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(28), StackDirection::BottomToTop, |tree| {
                        tree.widget(button_bottom_0.clone());
                        tree.widget(button_bottom_1.clone());
                        tree.widget(button_bottom_2.clone());
                    });
                });
            });
        });

        let weight_labels = self.weight_labels.clone();
        let weight_buttons = self.weight_buttons.clone();
        self.weight_tree = WidgetTreeBuilder::build(move |tree| {
            let [row_weight_label, grid_weight_label] = weight_labels.clone();
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
            ] = weight_buttons.clone();
            let row = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0), SizePolicy::Weight(3.0)];
            let cols = [SizePolicy::Weight(1.0), SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)];
            let rows = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)];
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                tree.widget(row_weight_label.clone());
            });
            tree.row(&row, SizePolicy::Fixed(28), |tree| {
                tree.widget(button_row_0.clone());
                tree.widget(button_row_1.clone());
                tree.widget(button_row_2.clone());
            });
            tree.row(&[SizePolicy::Weight(1.0)], SizePolicy::Auto, |tree| {
                tree.widget(grid_weight_label.clone());
            });
            tree.row(&[SizePolicy::Weight(1.0)], SizePolicy::Remainder(0), |tree| {
                tree.column(|tree| {
                    tree.grid(&cols, &rows, |tree| {
                        tree.widget(button_grid_0.clone());
                        tree.widget(button_grid_1.clone());
                        tree.widget(button_grid_2.clone());
                        tree.widget(button_grid_3.clone());
                        tree.widget(button_grid_4.clone());
                        tree.widget(button_grid_5.clone());
                    });
                });
            });
        });

        let combo_items = self.combo_items.clone();
        self.combo_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                for item in &combo_items {
                    tree.widget(item.clone());
                }
            });
        });

        let popup_buttons = self.popup_buttons.clone();
        self.popup_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                for button in &popup_buttons {
                    tree.widget(button.clone());
                }
            });
        });

        let window_header = self.window_header.clone();
        let test_buttons_header = self.test_buttons_header.clone();
        let background_header = self.background_header.clone();
        let tree_and_text_header = self.tree_and_text_header.clone();
        let text_area_header = self.text_area_header.clone();
        let slot_header = self.slot_header.clone();
        let combo_header = self.combo_header.clone();
        let test1_tn = self.test1_tn.clone();
        let test1a_tn = self.test1a_tn.clone();
        let test1b_tn = self.test1b_tn.clone();
        let test2_tn = self.test2_tn.clone();
        let test3_tn = self.test3_tn.clone();
        let window_info_labels = self.window_info_labels.clone();
        let window_info_values = self.window_info_values.clone();
        let test_buttons = self.test_buttons.clone();
        let test_button_labels = self.test_button_labels.clone();
        let combo_state = self.combo_state.clone();
        let tree_buttons = self.tree_buttons.clone();
        let checkboxes = self.checkboxes.clone();
        let tree_labels = self.tree_labels.clone();
        let text_area = self.text_area.clone();
        let bg_sliders = self.bg_sliders.clone();
        let background_labels = self.background_labels.clone();
        let background_swatch = self.background_swatch.clone();
        let slot_buttons = self.slot_buttons.clone();
        let external_image_button = self.external_image_button.clone();
        self.demo_tree = WidgetTreeBuilder::build(move |tree| {
            let window_info_row = [SizePolicy::Fixed(54), SizePolicy::Remainder(0)];
            let button_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
            let tree_widths = [SizePolicy::Fixed(140), SizePolicy::Remainder(0)];
            let tree_button_widths = [SizePolicy::Fixed(54), SizePolicy::Fixed(54)];
            let background_widths = [SizePolicy::Remainder(77), SizePolicy::Remainder(0)];
            let slider_row = [SizePolicy::Fixed(46), SizePolicy::Remainder(0)];
            let [label_pos, label_size, label_fps] = window_info_labels.clone();
            let [value_pos, value_size, value_fps] = window_info_values.clone();
            let [button0, button1, button2, button3, button4, button5] = test_buttons.clone();
            let [test_label0, test_label1, test_label2] = test_button_labels.clone();
            let [tree_button0, tree_button1, tree_button2, tree_button3, tree_button4, tree_button5] = tree_buttons.clone();
            let [checkbox0, checkbox1, checkbox2] = checkboxes.clone();
            let [tree_label_hello, tree_label_world] = tree_labels.clone();
            let [slider_red, slider_green, slider_blue] = bg_sliders.clone();
            let [label_red, label_green, label_blue] = background_labels.clone();
            let [slot0, slot1, slot2, slot3] = slot_buttons.clone();

            Self::section(tree, &window_header, |tree| {
                tree.row(&window_info_row, SizePolicy::Auto, |tree| {
                    tree.widget(label_pos.clone());
                    tree.widget(value_pos.clone());
                });
                tree.row(&window_info_row, SizePolicy::Auto, |tree| {
                    tree.widget(label_size.clone());
                    tree.widget(value_size.clone());
                });
                tree.row(&window_info_row, SizePolicy::Auto, |tree| {
                    tree.widget(label_fps.clone());
                    tree.widget(value_fps.clone());
                });
            });

            Self::section(tree, &test_buttons_header, |tree| {
                tree.row(&button_widths, SizePolicy::Auto, |tree| {
                    tree.widget(test_label0.clone());
                    tree.widget(button0.clone());
                    tree.widget(button1.clone());
                });
                tree.row(&button_widths, SizePolicy::Auto, |tree| {
                    tree.widget(test_label1.clone());
                    tree.widget(button2.clone());
                    tree.widget(button3.clone());
                });
                tree.row(&button_widths, SizePolicy::Auto, |tree| {
                    tree.widget(test_label2.clone());
                    tree.widget(button4.clone());
                    tree.widget(button5.clone());
                });
            });

            Self::section(tree, &combo_header, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                    tree.widget(combo_state.clone());
                });
            });

            Self::section(tree, &tree_and_text_header, |tree| {
                tree.row(&tree_widths, SizePolicy::Auto, |tree| {
                    tree.column(|tree| {
                        tree.tree_node(test1_tn.clone(), |tree| {
                            tree.tree_node(test1a_tn.clone(), |tree| {
                                tree.widget(tree_label_hello.clone());
                                tree.widget(tree_label_world.clone());
                            });
                            tree.tree_node(test1b_tn.clone(), |tree| {
                                tree.widget(tree_button0.clone());
                                tree.widget(tree_button1.clone());
                            });
                        });
                        tree.tree_node(test2_tn.clone(), |tree| {
                            tree.row(&tree_button_widths, SizePolicy::Auto, |tree| {
                                tree.widget(tree_button2.clone());
                                tree.widget(tree_button3.clone());
                            });
                            tree.row(&tree_button_widths, SizePolicy::Auto, |tree| {
                                tree.widget(tree_button4.clone());
                                tree.widget(tree_button5.clone());
                            });
                        });
                        tree.tree_node(test3_tn.clone(), |tree| {
                            tree.widget(checkbox0.clone());
                            tree.widget(checkbox1.clone());
                            tree.widget(checkbox2.clone());
                        });
                    });
                    tree.column(|tree| {
                        tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                            tree.text_with_wrap(
                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla.",
                                TextWrap::Word,
                            );
                        });
                    });
                });
            });

            Self::section(tree, &text_area_header, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(120), StackDirection::TopToBottom, |tree| {
                    tree.widget(text_area.clone());
                });
            });

            Self::section(tree, &background_header, |tree| {
                tree.row(&background_widths, SizePolicy::Fixed(74), |tree| {
                    tree.column(|tree| {
                        tree.row(&slider_row, SizePolicy::Auto, |tree| {
                            tree.widget(label_red.clone());
                            tree.widget(slider_red.clone());
                        });
                        tree.row(&slider_row, SizePolicy::Auto, |tree| {
                            tree.widget(label_green.clone());
                            tree.widget(slider_green.clone());
                        });
                        tree.row(&slider_row, SizePolicy::Auto, |tree| {
                            tree.widget(label_blue.clone());
                            tree.widget(slider_blue.clone());
                        });
                    });
                    tree.widget(background_swatch.clone());
                });
            });

            Self::section(tree, &slot_header, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(67), StackDirection::TopToBottom, |tree| {
                    tree.widget(slot0.clone());
                    tree.widget(slot1.clone());
                    tree.widget(slot2.clone());
                    if let Some(button) = external_image_button.clone() {
                        tree.stack(SizePolicy::Fixed(256), SizePolicy::Fixed(256), StackDirection::TopToBottom, |tree| {
                            tree.widget(button.clone());
                        });
                    }
                });
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(67), StackDirection::TopToBottom, |tree| {
                    tree.widget(slot3.clone());
                });
            });
        });
    }

    fn style_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        for (i, color) in self.style.colors.iter().enumerate() {
            let slider_base = i * 4;
            self.style_color_sliders[slider_base].borrow_mut().value = color.r as Real;
            self.style_color_sliders[slider_base + 1].borrow_mut().value = color.g as Real;
            self.style_color_sliders[slider_base + 2].borrow_mut().value = color.b as Real;
            self.style_color_sliders[slider_base + 3].borrow_mut().value = color.a as Real;
            self.style_color_swatches[i].borrow_mut().fill = *color;
        }
        self.style_value_sliders[0].borrow_mut().value = self.style.padding as Real;
        self.style_value_sliders[1].borrow_mut().value = self.style.spacing as Real;
        self.style_value_sliders[2].borrow_mut().value = self.style.title_height as Real;
        self.style_value_sliders[3].borrow_mut().value = self.style.thumb_size as Real;
        self.style_value_sliders[4].borrow_mut().value = self.style.scrollbar_size as Real;

        ctx.window(
            &mut self.style_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.style_tree,
        );

        for (color, sliders) in self.style.colors.iter_mut().zip(self.style_color_sliders.chunks_exact(4)) {
            color.r = sliders[0].borrow().value as u8;
            color.g = sliders[1].borrow().value as u8;
            color.b = sliders[2].borrow().value as u8;
            color.a = sliders[3].borrow().value as u8;
        }
        for (swatch, color) in self.style_color_swatches.iter().zip(self.style.colors.iter()) {
            swatch.borrow_mut().fill = *color;
        }
        self.style.padding = self.style_value_sliders[0].borrow().value as i32;
        self.style.spacing = self.style_value_sliders[1].borrow().value as i32;
        self.style.title_height = self.style_value_sliders[2].borrow().value as i32;
        self.style.thumb_size = self.style_value_sliders[3].borrow().value as i32;
        self.style.scrollbar_size = self.style_value_sliders[4].borrow().value as i32;
        ctx.set_style(&self.style);
    }

    fn log_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        self.log_text.borrow_mut().text = self.logbuf.borrow().clone();
        ctx.window(
            &mut self.log_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.log_tree,
        );

        if self.logbuf_updated {
            let mut log_output = self.log_output.as_mut().unwrap().clone();
            log_output.with_mut(|panel| {
                let mut scroll = panel.scroll();
                scroll.y = panel.content_size().height;
                panel.set_scroll(scroll);
            });
            self.logbuf_updated = false;
        }

        let mut submitted = false;
        {
            let results = ctx.committed_results();
            let submit_buf_out = results.state_of_handle(&self.submit_buf);
            let submit_btn_out = results.state_of_handle(&self.submit_button);
            if submit_buf_out.is_submitted() {
                self.log_window.as_mut().unwrap().set_focus(Some(widget_id_of_handle(&self.submit_buf)));
                submitted = true;
            }
            if submit_btn_out.is_submitted() {
                submitted = true;
            }
        }
        if submitted {
            let mut buf = String::new();
            buf.push_str(self.submit_buf.borrow().buf.as_str());
            self.write_log(buf.as_str());
            self.submit_buf.borrow_mut().buf.clear();
        }
    }

    fn typography_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.typography_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.typography_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.typography_tree,
        );
    }

    fn triangle_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.triangle_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.triangle_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.triangle_tree,
        );
    }

    fn suzane_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.suzane_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.suzane_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.suzane_tree,
        );
    }

    fn graphics_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.graphics_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.graphics_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.graphics_tree,
        );
    }

    fn falloff_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.falloff_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.falloff_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.falloff_tree,
        );
    }

    fn stack_direction_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.stack_direction_window.is_none() {
            return;
        }

        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.stack_direction_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.stack_direction_tree,
        );

        let results = ctx.committed_results();
        if results.state_of_handle(&self.stack_direction_buttons[0]).is_submitted() {
            logs.push("Top->Bottom: call 1");
        }
        if results.state_of_handle(&self.stack_direction_buttons[1]).is_submitted() {
            logs.push("Top->Bottom: call 2");
        }
        if results.state_of_handle(&self.stack_direction_buttons[2]).is_submitted() {
            logs.push("Top->Bottom: call 3");
        }
        if results.state_of_handle(&self.stack_direction_buttons[3]).is_submitted() {
            logs.push("Bottom->Top: call 1");
        }
        if results.state_of_handle(&self.stack_direction_buttons[4]).is_submitted() {
            logs.push("Bottom->Top: call 2");
        }
        if results.state_of_handle(&self.stack_direction_buttons[5]).is_submitted() {
            logs.push("Bottom->Top: call 3");
        }

        for msg in logs {
            self.write_log(msg);
        }
    }

    fn weight_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.weight_window.is_none() {
            return;
        }

        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.weight_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.weight_tree,
        );

        let results = ctx.committed_results();
        if results.state_of_handle(&self.weight_buttons[0]).is_submitted() {
            logs.push("Weight row: 1");
        }
        if results.state_of_handle(&self.weight_buttons[1]).is_submitted() {
            logs.push("Weight row: 2");
        }
        if results.state_of_handle(&self.weight_buttons[2]).is_submitted() {
            logs.push("Weight row: 3");
        }
        if results.state_of_handle(&self.weight_buttons[3]).is_submitted() {
            logs.push("Weight grid: 1");
        }
        if results.state_of_handle(&self.weight_buttons[4]).is_submitted() {
            logs.push("Weight grid: 2");
        }
        if results.state_of_handle(&self.weight_buttons[5]).is_submitted() {
            logs.push("Weight grid: 3");
        }
        if results.state_of_handle(&self.weight_buttons[6]).is_submitted() {
            logs.push("Weight grid: 4");
        }
        if results.state_of_handle(&self.weight_buttons[7]).is_submitted() {
            logs.push("Weight grid: 5");
        }
        if results.state_of_handle(&self.weight_buttons[8]).is_submitted() {
            logs.push("Weight grid: 6");
        }

        for msg in logs {
            self.write_log(msg);
        }
    }

    fn test_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        {
            let window = self.demo_window.as_mut().unwrap();
            let mut win = window.rect();
            win.width = win.width.max(240);
            win.height = win.height.max(300);
            window.set_rect(win);

            let [value_pos, value_size, value_fps] = self.window_info_values.clone();
            value_pos.borrow_mut().label = format!("{}, {}", win.x, win.y);
            value_size.borrow_mut().label = format!("{}, {}", win.width, win.height);
            value_fps.borrow_mut().label = format!("{:.1}", self.fps);
        }

        let combo_labels: Vec<String> = self.combo_items.iter().map(|item| item.borrow().label.clone()).collect();
        self.combo_state.borrow_mut().update_items(&combo_labels);

        self.bg_sliders[0].borrow_mut().value = self.bg[0];
        self.bg_sliders[1].borrow_mut().value = self.bg[1];
        self.bg_sliders[2].borrow_mut().value = self.bg[2];
        {
            let mut swatch = self.background_swatch.borrow_mut();
            swatch.fill = color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255);
            swatch.label = format!("#{:02X}{:02X}{:02X}", swatch.fill.r, swatch.fill.g, swatch.fill.b);
        }

        ctx.window(
            &mut self.demo_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.demo_tree,
        );

        let mut button_logs: Vec<&'static str> = Vec::new();
        let mut tree_logs: Vec<&'static str> = Vec::new();
        let combo_anchor = self.combo_state.borrow().anchor();
        {
            let results = ctx.committed_results();
            if results.state_of_handle(&self.test_buttons[0]).is_submitted() {
                button_logs.push("Pressed button 1");
            }
            if results.state_of_handle(&self.test_buttons[1]).is_submitted() {
                button_logs.push("Pressed button 2");
            }
            if results.state_of_handle(&self.test_buttons[2]).is_submitted() {
                button_logs.push("Pressed button 3");
            }
            if results.state_of_handle(&self.test_buttons[3]).is_submitted() {
                self.open_popup = true;
            }
            if results.state_of_handle(&self.test_buttons[4]).is_submitted() {
                button_logs.push("Pressed button 4");
            }
            if results.state_of_handle(&self.test_buttons[5]).is_submitted() {
                self.open_dialog = true;
            }
            if results.state_of_handle(&self.tree_buttons[0]).is_submitted() {
                tree_logs.push("Pressed button 1");
            }
            if results.state_of_handle(&self.tree_buttons[1]).is_submitted() {
                tree_logs.push("Pressed button 2");
            }
            if results.state_of_handle(&self.tree_buttons[2]).is_submitted() {
                tree_logs.push("Pressed button 3");
            }
            if results.state_of_handle(&self.tree_buttons[3]).is_submitted() {
                tree_logs.push("Pressed button 4");
            }
            if results.state_of_handle(&self.tree_buttons[4]).is_submitted() {
                tree_logs.push("Pressed button 5");
            }
            if results.state_of_handle(&self.tree_buttons[5]).is_submitted() {
                tree_logs.push("Pressed button 6");
            }
        }
        self.bg[0] = self.bg_sliders[0].borrow().value;
        self.bg[1] = self.bg_sliders[1].borrow().value;
        self.bg[2] = self.bg_sliders[2].borrow().value;
        {
            let mut swatch = self.background_swatch.borrow_mut();
            swatch.fill = color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255);
            swatch.label = format!("#{:02X}{:02X}{:02X}", swatch.fill.r, swatch.fill.g, swatch.fill.b);
        }
        for msg in button_logs {
            self.write_log(msg);
        }
        for msg in tree_logs {
            self.write_log(msg);
        }

        let mut popup = self.combo_state.borrow().popup.clone();
        if self.combo_state.borrow().is_open() {
            ctx.open_popup_at(&mut popup, combo_anchor);
        }

        ctx.popup(&mut popup, WidgetBehaviourOption::NO_SCROLL, &self.combo_tree);
        let combo_log = {
            let results = ctx.committed_results();
            let mut selected_label = None;
            for (idx, item) in self.combo_items.iter().enumerate() {
                if results.state_of_handle(item).is_submitted() {
                    selected_label = self.combo_state.borrow_mut().select(idx, &combo_labels);
                    break;
                }
            }
            selected_label
        };
        if let Some(label) = combo_log {
            let msg = format!("Selected: {label}");
            self.write_log(msg.as_str());
        }

        if self.open_popup {
            let popup_width = (self.style.default_cell_width + self.style.padding.max(0) * 2).max(80);
            let popup = self.popup_window.as_mut().unwrap();
            ctx.open_popup(popup);
            popup.set_size(&Dimensioni::new(popup_width, 1));
            self.open_popup = false;
        }

        let mut popup_logs: Vec<&'static str> = Vec::new();
        ctx.popup(
            &mut self.popup_window.as_mut().unwrap().clone(),
            WidgetBehaviourOption::NO_SCROLL,
            &self.popup_tree,
        );
        {
            let results = ctx.committed_results();
            if results.state_of_handle(&self.popup_buttons[0]).is_submitted() {
                popup_logs.push("Hello")
            }
            if results.state_of_handle(&self.popup_buttons[1]).is_submitted() {
                popup_logs.push("World")
            }
        }
        for msg in popup_logs {
            self.write_log(msg);
        }

        self.dialog(ctx);
    }

    fn dialog(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.open_dialog {
            self.dialog_window.as_mut().unwrap().open(ctx);
            self.open_dialog = false;
            self.write_log("Open dialog!");
        }

        let dialog_result = {
            let dialog = self.dialog_window.as_mut().unwrap();
            let was_open = dialog.is_open();
            dialog.eval(ctx);
            if was_open && !dialog.is_open() {
                Some(dialog.file_name().clone())
            } else {
                None
            }
        };
        if let Some(result) = dialog_result {
            match result {
                Some(name) => {
                    let mut msg = String::new();
                    msg.push_str("Selected file: ");
                    msg.push_str(name.as_str());
                    self.write_log(msg.as_str());
                }
                None => self.write_log("File dialog canceled"),
            }
        }
    }

    fn process_frame(&mut self, ctx: &mut Context<BackendRenderer>) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if dt > 0.0 {
            let inst_fps = 1.0 / dt;
            self.fps = if self.fps == 0.0 { inst_fps } else { self.fps * 0.9 + inst_fps * 0.1 };
        }

        ctx.frame(|ctx| {
            self.style_window(ctx);
            self.log_window(ctx);
            self.typography_window(ctx);
            self.test_window(ctx);
            self.triangle_window(ctx);
            self.graphics_window(ctx);
            self.falloff_window(ctx);
            self.suzane_window(ctx);
            self.stack_direction_window(ctx);
            self.weight_window(ctx);
        })
    }
}

fn main() {
    let slots_orig = atlas_assets::default_slots();
    let mut atlas = atlas_assets::load_atlas(&slots_orig);
    let slots = atlas.clone_slot_table();
    atlas.render_slot(slots[0], Rc::new(|_x, _y| color4b(0xFF, 0, 0, 0xFF)));
    atlas.render_slot(slots[1], Rc::new(|_x, _y| color4b(0, 0xFF, 0, 0xFF)));
    atlas.render_slot(slots[2], Rc::new(|_x, _y| color4b(0, 0, 0xFF, 0xFF)));
    #[cfg(feature = "builder")]
    {
        builder::Builder::save_png_image(atlas.clone(), "atlas.png").unwrap();
    }

    let mut app = Application::new(atlas.clone(), move |backend: BackendInitContext, ctx| {
        let slots = atlas.clone_slot_table();
        let renderer = ctx.renderer_handle();
        State::new(backend, renderer, slots, ctx)
    })
    .unwrap();

    app.event_loop(|ctx, state| {
        state.process_frame(ctx);
    });
}

fn area_from_args(args: &CustomRenderArgs) -> CustomRenderArea {
    let clip = args
        .content_area
        .intersect(&args.view)
        .unwrap_or_else(|| rect(args.content_area.x, args.content_area.y, 0, 0));
    CustomRenderArea { rect: args.content_area, clip }
}

fn stroke_graphics_rect(graphics: &mut Graphics<'_, '_>, rect: Recti, width: f32, color: Color) {
    if rect.width <= 0 || rect.height <= 0 {
        return;
    }

    let x0 = rect.x as f32;
    let y0 = rect.y as f32;
    let x1 = (rect.x + rect.width) as f32;
    let y1 = (rect.y + rect.height) as f32;
    graphics.stroke_line(Vec2f::new(x0, y0), Vec2f::new(x1, y0), width, color);
    graphics.stroke_line(Vec2f::new(x1, y0), Vec2f::new(x1, y1), width, color);
    graphics.stroke_line(Vec2f::new(x1, y1), Vec2f::new(x0, y1), width, color);
    graphics.stroke_line(Vec2f::new(x0, y1), Vec2f::new(x0, y0), width, color);
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
fn load_external_image_texture(ctx: &mut Context<BackendRenderer>) -> Option<TextureId> {
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
