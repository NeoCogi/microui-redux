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

use microui_redux::*;
use miniquad::*;

const MAX_VERTEX_COUNT: usize = 65536;
const MAX_INDEX_COUNT: usize = 65536;

const VERTEX_SHADER: &str = "#version 100
uniform highp mat4 uTransform;
attribute highp vec2 vertexPosition;
attribute highp vec2 vertexTexCoord;
attribute lowp vec4 vertexColor;
varying highp vec2 vTexCoord;
varying lowp vec4 vVertexColor;
void main()
{
    vVertexColor = vertexColor / 255.0;
    vTexCoord = vertexTexCoord;
    highp vec4 pos = vec4(vertexPosition.x, vertexPosition.y, 0.0, 1.0);
    gl_Position = uTransform * pos;
}";

const FRAGMENT_SHADER: &str = "#version 100
varying highp vec2 vTexCoord;
varying lowp vec4 vVertexColor;
uniform sampler2D uTexture;
void main()
{
    lowp vec4 col = texture2D(uTexture, vTexCoord);
    gl_FragColor = col * vVertexColor;
}";

pub struct MQRenderer {
    ctx: Box<dyn RenderingBackend>,
    verts: Vec<Vertex>,
    indices: Vec<u16>,

    vbo: BufferId,
    ibo: BufferId,
    tex_o: TextureId,

    program: ShaderId,
    bindings: Bindings,
    pipeline: Pipeline,

    width: u32,
    height: u32,

    atlas: AtlasHandle,
    last_update_id: usize,
}

impl Drop for MQRenderer {
    fn drop(&mut self) {
        self.ctx.delete_buffer(self.vbo);
        self.ctx.delete_buffer(self.ibo);
        self.ctx.delete_texture(self.tex_o);
        self.ctx.delete_shader(self.program);
    }
}

impl MQRenderer {
    fn update_atlas(&mut self) {
        if self.last_update_id != self.atlas.get_last_update_id() {
            self.ctx.texture_update(
                self.tex_o,
                &self.atlas.pixels().iter().map(|c| [c.x, c.y, c.z, c.w]).flatten().collect::<Vec<u8>>(),
            );
            self.last_update_id = self.atlas.get_last_update_id()
        }
    }

    pub fn new(mut ctx: Box<dyn RenderingBackend>, atlas: AtlasHandle, width: u32, height: u32) -> Self {
        assert_eq!(core::mem::size_of::<Vertex>(), 20);

        let tex_params = TextureParams {
            kind: TextureKind::Texture2D,
            format: TextureFormat::RGBA8,
            wrap: TextureWrap::Clamp,
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            mipmap_filter: MipmapFilterMode::None,
            width: atlas.width() as _,
            height: atlas.height() as _,
            allocate_mipmaps: false,
        };
        let bytes = atlas.pixels().iter().map(|c| [c.x, c.y, c.z, c.w]).flatten().collect::<Vec<u8>>();
        let tex_o = ctx.new_texture_from_data_and_format(&bytes, tex_params);

        let vertex_size = core::mem::size_of::<Vertex>();
        let vbo = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Dynamic,
            BufferSource::Empty {
                size: vertex_size * MAX_VERTEX_COUNT,
                element_size: vertex_size,
            },
        );
        let ibo = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Dynamic,
            BufferSource::Empty {
                size: 2 * MAX_INDEX_COUNT,
                element_size: 2,
            },
        );

        let shader = ShaderSource::Glsl {
            vertex: VERTEX_SHADER,
            fragment: FRAGMENT_SHADER,
        };
        let program = ctx
            .new_shader(
                shader,
                ShaderMeta {
                    images: vec!["uTexture".to_string()],
                    uniforms: UniformBlockLayout {
                        uniforms: vec![UniformDesc::new("uTransform", UniformType::Mat4)],
                    },
                },
            )
            .unwrap();

        let bindings = Bindings {
            vertex_buffers: vec![vbo],
            index_buffer: ibo,
            images: vec![tex_o],
        };

        let pipeline = ctx.new_pipeline(
            &[BufferLayout {
                stride: 20,
                step_func: VertexStep::PerVertex,
                step_rate: 1,
            }],
            &[
                VertexAttribute::new("vertexPosition", VertexFormat::Float2),
                VertexAttribute::new("vertexTexCoord", VertexFormat::Float2),
                VertexAttribute::new("vertexColor", VertexFormat::Byte4),
            ],
            program,
            PipelineParams {
                cull_face: CullFace::Nothing,
                front_face_order: FrontFaceOrder::CounterClockwise,
                depth_test: Comparison::Never,
                depth_write: false,
                depth_write_offset: None,
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                alpha_blend: Some(BlendState::new(Equation::Add, BlendFactor::Zero, BlendFactor::One)),
                stencil_test: None,
                color_write: (true, true, true, true),
                primitive_type: PrimitiveType::Triangles,
            },
        );
        Self {
            ctx,
            verts: Vec::new(),
            indices: Vec::new(),

            vbo,
            ibo,
            tex_o,
            program,
            bindings,
            pipeline,

            width,
            height,
            atlas,
            last_update_id: usize::MAX,
        }
    }
}

impl Renderer for MQRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn flush(&mut self) {
        self.update_atlas();
        if self.verts.len() == 0 || self.indices.len() == 0 {
            return;
        }

        let ortho = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);

        self.ctx.buffer_update(self.vbo, BufferSource::slice(&self.verts));
        self.ctx.buffer_update(self.ibo, BufferSource::slice(&self.indices));
        self.ctx.apply_viewport(0, 0, self.width as i32, self.height as i32);
        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);
        self.ctx.apply_uniforms(UniformsSource::table(&ortho));
        self.ctx.draw(0, self.indices.len() as i32, 1);

        self.verts.clear();
        self.indices.clear();
    }

    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        if self.verts.len() + 4 >= MAX_VERTEX_COUNT || self.indices.len() + 6 >= MAX_INDEX_COUNT {
            self.flush();
        }

        let is = self.verts.len() as u16;
        self.indices.push(is + 0);
        self.indices.push(is + 1);
        self.indices.push(is + 2);
        self.indices.push(is + 2);
        self.indices.push(is + 3);
        self.indices.push(is + 0);

        self.verts.push(v0.clone());
        self.verts.push(v1.clone());
        self.verts.push(v2.clone());
        self.verts.push(v3.clone());

        // This is needed so that the update happens immediately (not the most optimized way)
        if self.last_update_id != self.atlas.get_last_update_id() {
            self.flush()
        }
    }

    fn begin(&mut self, width: i32, height: i32, clr: Color) {
        self.width = width as u32;
        self.height = height as u32;
        self.flush();
        self.ctx.begin_default_pass(PassAction::Clear {
            color: Some((clr.r as f32 / 255.0, clr.g as f32 / 255.0, clr.b as f32 / 255.0, clr.a as f32 / 255.0)),
            depth: Some(1.0),
            stencil: None,
        });
    }

    fn end(&mut self) {
        self.flush();
        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }
}
