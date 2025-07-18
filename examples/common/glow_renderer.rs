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

use core::slice;
use std::{sync::Arc, usize};

use super::*;
use glow::*;

const VERTEX_SHADER: &str = "#version 100
uniform highp mat4 uTransform;
attribute highp vec2 vertexPosition;
attribute highp vec2 vertexTexCoord;
attribute lowp vec4 vertexColor;
varying highp vec2 vTexCoord;
varying lowp vec4 vVertexColor;
void main()
{
    vVertexColor = vertexColor;
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

pub struct GLRenderer {
    gl: Arc<glow::Context>,
    verts: Vec<Vertex>,
    indices: Vec<u16>,

    vbo: NativeBuffer,
    ibo: NativeBuffer,
    tex_o: NativeTexture,

    program: NativeProgram,

    width: u32,
    height: u32,

    atlas: AtlasHandle,
    last_update_id: usize,
}

impl GLRenderer {
    fn update_atlas(&mut self) {
        let gl = &self.gl;
        if self.last_update_id != self.atlas.get_last_update_id() {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                debug_assert!(gl.get_error() == 0);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                debug_assert!(gl.get_error() == 0);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, 0);
                debug_assert!(gl.get_error() == 0);

                // we are going to pass a pointer, hold the atlas pixels in memory since it returns a copy
                self.atlas.apply_pixels(|width, height, pixels| {
                    let pixel_ptr = pixels.as_ptr() as *const u8;
                    let pixel_slice: &[u8] = slice::from_raw_parts(pixel_ptr, pixels.len() * 4);
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA as i32,
                        width as i32,
                        height as i32,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        PixelUnpackData::Slice(Some(pixel_slice)),
                    );
                    debug_assert!(gl.get_error() == 0);
                });
            }
            self.last_update_id = self.atlas.get_last_update_id()
        }
    }

    pub fn new(gl: Arc<glow::Context>, atlas: AtlasHandle, width: u32, height: u32) -> Self {
        assert_eq!(core::mem::size_of::<Vertex>(), 32);
        unsafe {
            // init texture
            let tex_o = gl.create_texture().unwrap();
            debug_assert!(gl.get_error() == 0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex_o));
            debug_assert!(gl.get_error() == 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            debug_assert!(gl.get_error() == 0);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            debug_assert!(gl.get_error() == 0);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);

            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                atlas.width() as i32,
                atlas.height() as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(None),
            );
            debug_assert!(gl.get_error() == 0);
            gl.bind_texture(glow::TEXTURE_2D, None);

            let vbo = gl.create_buffer().unwrap();
            let ibo = gl.create_buffer().unwrap();

            let program = glow_common::create_program(&gl, VERTEX_SHADER, FRAGMENT_SHADER).unwrap();

            Self {
                gl,
                verts: Vec::new(),
                indices: Vec::new(),

                vbo,
                ibo,
                tex_o,
                program,

                width,
                height,
                atlas,
                last_update_id: usize::MAX,
            }
        }
    }
}

impl Renderer for GLRenderer {
    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn flush(&mut self) {
        self.update_atlas();
        if self.verts.len() == 0 || self.indices.len() == 0 {
            return;
        }

        let gl = &self.gl;
        unsafe {
            // opengl rendering states
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.scissor(0, 0, self.width as i32, self.height as i32);
            gl.enable(glow::BLEND);
            debug_assert!(gl.get_error() == 0);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::CULL_FACE);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::DEPTH_TEST);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::SCISSOR_TEST);
            debug_assert!(gl.get_error() == 0);

            // set the program
            gl.use_program(Some(self.program));
            debug_assert!(gl.get_error() == 0);

            // set the texture
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_o));
            gl.active_texture(glow::TEXTURE0 + 0);
            let tex_uniform_id = gl.get_uniform_location(self.program, "uTexture").unwrap();
            gl.uniform_1_i32(Some(&tex_uniform_id), 0);
            debug_assert_eq!(gl.get_error(), 0);

            // set the viewport
            let viewport = gl.get_uniform_location(self.program, "uTransform").unwrap();
            let tm = ortho4(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
            let tm_ptr = tm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&viewport), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            // set the vertex buffer
            let pos_attrib_id = gl.get_attrib_location(self.program, "vertexPosition").unwrap();
            let tex_attrib_id = gl.get_attrib_location(self.program, "vertexTexCoord").unwrap();
            let col_attrib_id = gl.get_attrib_location(self.program, "vertexColor").unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ibo));
            debug_assert!(gl.get_error() == 0);

            // update the vertex buffer
            let vertices_u8: &[u8] = core::slice::from_raw_parts(self.verts.as_ptr() as *const u8, self.verts.len() * core::mem::size_of::<Vertex>());
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
            debug_assert!(gl.get_error() == 0);

            // update the index buffer
            let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u16>());
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
            debug_assert!(gl.get_error() == 0);

            gl.enable_vertex_attrib_array(pos_attrib_id);
            gl.enable_vertex_attrib_array(tex_attrib_id);
            gl.enable_vertex_attrib_array(col_attrib_id);
            debug_assert!(gl.get_error() == 0);

            gl.vertex_attrib_pointer_f32(pos_attrib_id, 2, glow::FLOAT, false, 32, 0);
            gl.vertex_attrib_pointer_f32(tex_attrib_id, 2, glow::FLOAT, false, 32, 8);
            gl.vertex_attrib_pointer_f32(col_attrib_id, 4, glow::FLOAT, false, 32, 16);
            debug_assert!(gl.get_error() == 0);

            gl.draw_elements(glow::TRIANGLES, self.indices.len() as i32, glow::UNSIGNED_SHORT, 0);
            debug_assert!(gl.get_error() == 0);

            gl.disable_vertex_attrib_array(pos_attrib_id);
            gl.disable_vertex_attrib_array(tex_attrib_id);
            gl.disable_vertex_attrib_array(col_attrib_id);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);

            self.verts.clear();
            self.indices.clear();
        }
    }

    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex) {
        if self.verts.len() + 4 >= 65536 || self.indices.len() + 6 >= 65536 {
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
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.scissor(0, 0, self.width as i32, self.height as i32);
            gl.clear_color(clr.r as f32 / 255.0, clr.g as f32 / 255.0, clr.b as f32 / 255.0, clr.a as f32 / 255.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            debug_assert!(gl.get_error() == 0);
        }
    }

    fn end(&mut self) {
        self.flush();
    }
}
