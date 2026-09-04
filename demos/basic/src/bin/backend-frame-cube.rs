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
//! A small rotating cube that demonstrates typed backend-frame custom rendering.
//!
//! The cube geometry is intentionally small, while `rs_math3d` supplies its vectors, quaternion
//! rotation, view/projection matrices, and screen projection. The important part of the example
//! is the ownership path used to submit the resulting triangles:
//!
//! 1. one Cargo feature selects a concrete renderer as `SelectedBackend`;
//! 2. `SelectedFrame<'a>` names that backend's `RendererBackend::Frame<'a>` GAT;
//! 3. a retained `CubeWidget` reserves layout space and provides normal widget state;
//! 4. a typed custom-render callback receives the already-active `&mut SelectedFrame<'_>`;
//! 5. the callback calls an inherent method implemented by all three example frame types.
//!
//! No backend handle is captured and no second frame is opened. The supplied frame already owns
//! the backend's exclusive mutable borrow and is finalized by `Drop` after display-list execution.
//!
//! Run with one backend, for example:
//!
//! ```text
//! cargo run -p microui-redux-basic-demos --bin backend-frame-cube --no-default-features --features glow
//! cargo run -p microui-redux-basic-demos --bin backend-frame-cube --no-default-features --features vulkan
//! cargo run -p microui-redux-basic-demos --bin backend-frame-cube --no-default-features --features wgpu
//! ```

use microui_redux_demo_assets as atlas_assets;
use microui_redux_demo_host::{Application, SelectedBackend};
use microui_redux_renderer_common::CustomRenderArea;
use microui_redux::{prelude::*, render::Vertex};
use rs_math3d::{EPS_F32, lookat, perspective, project3};
use std::cell::Cell;
use std::rc::Rc;

/// Resolves the concrete active-frame type chosen by the backend Cargo feature.
///
/// The lifetime is tied to the exclusive borrow made by `RendererBackend::frame`. The callback
/// may use the frame for this invocation but cannot retain it after the callback returns.
type SelectedFrame<'a> = <SelectedBackend as RendererBackend>::Frame<'a>;

/// Minimal persistent widget state for the custom-render node.
///
/// Rendering is intentionally absent from `paint`: the typed callback registered on `Context`
/// records the cube at the correct display-list ordering point. A real interactive viewport could
/// process mouse input in `update` and share camera state with that callback.
struct CubeWidget {
    opt: WidgetOption,
}

struct CubeParameters;

impl WidgetParameters for CubeParameters {}

struct CubeBuilder;

impl WidgetBuilder for CubeBuilder {
    type Parameters = CubeParameters;
    type W = CubeWidget;

    fn create_widget(_parameters: Self::Parameters) -> Self::W {
        CubeWidget { opt: WidgetOption::NO_INTERACT }
    }
}

impl Widget for CubeWidget {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        // The custom-render callback is the paint path for this node.
    }
}

impl LeafWidget for CubeWidget {
    fn measure(&self, _style: &Skin, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(300, 300)
    }
}

const CUBE_POINTS: [Vec3f; 8] = [
    Vec3f { x: -1.0, y: -1.0, z: -1.0 },
    Vec3f { x: 1.0, y: -1.0, z: -1.0 },
    Vec3f { x: 1.0, y: 1.0, z: -1.0 },
    Vec3f { x: -1.0, y: 1.0, z: -1.0 },
    Vec3f { x: -1.0, y: -1.0, z: 1.0 },
    Vec3f { x: 1.0, y: -1.0, z: 1.0 },
    Vec3f { x: 1.0, y: 1.0, z: 1.0 },
    Vec3f { x: -1.0, y: 1.0, z: 1.0 },
];

// Each quad is emitted as two triangles. Opaque faces are sorted from far to near because this
// simple example deliberately uses the UI colored-triangle path rather than a depth buffer.
const CUBE_FACES: [([usize; 4], Color4b); 6] = [
    ([0, 1, 2, 3], Color4b { x: 70, y: 105, z: 210, w: 255 }),
    ([4, 5, 6, 7], Color4b { x: 220, y: 82, z: 82, w: 255 }),
    ([0, 4, 7, 3], Color4b { x: 72, y: 180, z: 112, w: 255 }),
    ([1, 5, 6, 2], Color4b { x: 230, y: 172, z: 62, w: 255 }),
    ([0, 1, 5, 4], Color4b { x: 142, y: 92, z: 205, w: 255 }),
    ([3, 2, 6, 7], Color4b { x: 60, y: 185, z: 205, w: 255 }),
];

fn face_depth(corners: &[usize; 4], points: &[Vec3f; 8]) -> f32 {
    corners.iter().map(|&index| points[index].z).sum::<f32>() / 4.0
}

/// CPU-projects a cube into screen-space `Vertex` triangles.
///
/// `enqueue_colored_vertices` uses the normal UI shader, so every vertex points at the center of
/// the atlas's white icon and its packed color supplies the visible face color.
fn build_cube_vertices(area: Recti, white_uv: Vec2f, angle: f32) -> Vec<Vertex> {
    let rotate_y = Quatf::of_axis_angle(&Vec3f::new(0.0, 1.0, 0.0), angle, EPS_F32).expect("valid Y rotation");
    let rotate_x = Quatf::of_axis_angle(&Vec3f::new(1.0, 0.0, 0.0), angle * 0.63, EPS_F32).expect("valid X rotation");
    let model = (rotate_x * rotate_y).mat4();
    let view = lookat(&Vec3f::new(0.0, 0.0, 4.0), &Vec3f::new(0.0, 0.0, 0.0), &Vec3f::new(0.0, 1.0, 0.0));
    let projection = perspective(std::f32::consts::FRAC_PI_3, area.width.max(1) as f32 / area.height.max(1) as f32, 0.1, 100.0);
    let model_view = view * model;

    // `project3` uses a bottom-left screen origin. Reversing the Y endpoints maps its result to
    // microui's top-left screen coordinates without hand-writing another projection transform.
    let screen_bottom_left = Vec2f::new(area.x as f32, (area.y + area.height) as f32);
    let screen_top_right = Vec2f::new((area.x + area.width) as f32, area.y as f32);
    let projected = CUBE_POINTS.map(|point| project3(&model_view, &projection, &screen_bottom_left, &screen_top_right, &point));

    let mut faces = CUBE_FACES;
    faces.sort_by(|(left, _), (right, _)| face_depth(right, &projected).total_cmp(&face_depth(left, &projected)));

    let mut vertices = Vec::with_capacity(6 * 6);
    for (corners, color) in faces {
        let [a, b, c, d] = corners;
        for index in [a, b, c, a, c, d] {
            vertices.push(Vertex::new(Vec2f::new(projected[index].x, projected[index].y), white_uv, color));
        }
    }
    vertices
}

/// Returns the atlas coordinate sampled by solid colored triangles.
fn white_uv(atlas: &AtlasHandle) -> Vec2f {
    let white = atlas.get_icon_rect(atlas.white_icon());
    let texture = atlas.get_texture_dimension();
    Vec2f::new(
        (white.x as f32 + white.width as f32 * 0.5) / texture.width.max(1) as f32,
        (white.y as f32 + white.height as f32 * 0.5) / texture.height.max(1) as f32,
    )
}

struct State {
    /// Shared application state is updated before `Context::frame` and read by the callback later
    /// during display-list execution. Only the angle is shared; the frame itself never is.
    angle: Rc<Cell<f32>>,
}

fn main() {
    let atlas = atlas_assets::load_atlas();

    let mut app = Application::new(atlas, |_backend, ctx| {
        let angle = Rc::new(Cell::new(0.0_f32));
        let callback_angle = angle.clone();
        let white_uv = white_uv(&ctx.atlas());

        let cube_renderer = ctx
            .register_custom_renderer(move |frame: &mut SelectedFrame<'_>, args: CustomRenderArgs| {
                // Context's executor invokes this callback only for the authoritative visible view.
                let area = CustomRenderArea { rect: args.content_area, clip: args.view };
                let vertices = build_cube_vertices(args.content_area, white_uv, callback_angle.get());

                // This method is specific to the concrete example frame types. It is available
                // here precisely because the callback receives `SelectedFrame`, not `dyn
                // RendererFrame` and not an opaque backend handle.
                frame.enqueue_colored_vertices(area, vertices);
            })
            .expect("register cube renderer");

        let cube = CubeBuilder::create_widget(CubeParameters);
        let tree = Node::custom_render(cube, cube_renderer);
        ctx.ui().create_window(Window::new("Typed backend-frame cube", rect(40, 40, 360, 360), tree));

        State { angle }
    })
    .expect("initialize cube example");

    app.event_loop(|_ctx, state, _dimensions| {
        // Application state changes before Application creates ContextFrame. The typed callback
        // reads this value later while Context executes that frame's custom-render command.
        state.angle.set((state.angle.get() + 0.018) % std::f32::consts::TAU);
    });
}
