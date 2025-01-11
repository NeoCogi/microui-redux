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
// TODO: Triangulate inputs
// TODO: have the polygon points to 2 arrays: vertices and triangles
//
use std::collections::*;
use super::*;
use glow::*;
use glow_common::*;

use std::iter::*;

#[repr(C)]
pub struct Vertex {
    position: Vec3f,
    normal: Vec3f,
    uv: Vec2f,
}

#[derive(Clone)]
struct Polygon {
    len: usize,
    start: usize,
}

#[derive(Copy, Clone)]
pub struct PolyVertex {
    pub pos: usize,
    pub normal: usize,
    pub tex: usize,
}

#[derive(Copy, Clone)]
pub struct PMVertex {
    position: Vec3f,
    selected: bool,
}

#[derive(Clone)]
pub struct PolyMesh {
    bbox: Box3f,
    v_positions: Vec<PMVertex>,
    v_normals: Vec<Vec3f>,
    v_tex: Vec<Vec2f>,

    vertices: Vec<PolyVertex>,
    polys: Vec<Polygon>,
}

impl<'a> PolyMesh {
    pub fn new() -> Self {
        let mut bbox = Box3f::new(&Vec3f::zero(), &Vec3f::zero());
        bbox.min = Vec3f::new(f32::MAX, f32::MAX, f32::MAX);
        bbox.max = Vec3f::new(-f32::MAX, -f32::MAX, -f32::MAX);
        Self {
            bbox,

            v_positions: Vec::new(),
            v_normals: Vec::new(),
            v_tex: Vec::new(),

            vertices: Vec::new(),
            polys: Vec::new(),
        }
    }
    pub fn poly_count(&self) -> usize {
        self.polys.len()
    }
    pub fn get_poly(&'a self, f: usize) -> PolygonIterator<'a> {
        PolygonIterator {
            mesh: self,
            poly_id: f,
            v_count: self.polys[f].len,
            v_id: 0,
        }
    }

    pub fn calculate_bounding_box(&self) -> Box3f {
        self.polys().fold(Box3f::new(&Vec3f::zero(), &Vec3f::zero()), |mut b, p| {
            for v in p {
                b.min = Vector3::min(&b.min, &self.v_positions[v.pos].position);
                b.max = Vector3::max(&b.max, &self.v_positions[v.pos].position);
            }
            b
        })
    }

    pub fn set_vertices(&mut self, positions: Vec<Vec3f>, normals: Vec<Vec3f>, tex: Vec<Vec2f>) {
        self.v_positions = positions.into_iter().map(|position| PMVertex { position, selected: false }).collect();
        self.v_normals = normals;
        self.v_tex = tex;
    }

    pub fn add_poly(&mut self, verts: &Vec<PolyVertex>) {
        let len = verts.len();
        let start = self.vertices.len();

        let mut norm_verts = [Vec3f::zero(); 3];
        let mut i = 0;
        for v in verts {
            if i < 3 {
                norm_verts[i] = self.v_positions[v.pos].position;
            }
            self.vertices.push(v.clone());
            self.bbox.add(&self.v_positions[v.pos].position);
            i += 1;
        }

        self.polys.push(Polygon { len, start });
    }

    pub fn polys(&'a self) -> PolyMeshIterator<'a> {
        PolyMeshIterator { mesh: self, poly_id: 0 }
    }
}

#[derive(Clone, Copy)]
pub struct PolygonIterator<'a> {
    mesh: &'a PolyMesh,
    poly_id: usize,
    v_count: usize,
    v_id: usize,
}

impl<'a> Iterator for PolygonIterator<'a> {
    type Item = &'a PolyVertex;
    fn next(&mut self) -> Option<Self::Item> {
        if self.v_id < self.v_count {
            let v = &self.mesh.vertices[self.mesh.polys[self.poly_id].start + self.v_id];
            self.v_id += 1;
            Some(v)
        } else {
            None
        }
    }
}

impl<'a> PolygonIterator<'a> {
    pub fn vertex_count(&self) -> usize {
        self.mesh.polys[self.poly_id].len
    }
}

pub struct PolyMeshIterator<'a> {
    mesh: &'a PolyMesh,
    poly_id: usize,
}

impl<'a> Iterator for PolyMeshIterator<'a> {
    type Item = PolygonIterator<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.poly_id < self.mesh.poly_count() {
            let p = self.mesh.get_poly(self.poly_id);
            self.poly_id += 1;
            Some(p)
        } else {
            None
        }
    }
}

static SOLID_VERTEX_SHADER: &'static str = "#version 100
uniform highp mat4 pvm;
attribute highp vec3 position;
void main()
{
    highp vec4 pos = vec4(position.xyz, 1.0);
    gl_Position = pvm * pos;
}";

static SOLID_PIXEL_SHADER: &'static str = "#version 100
void main()
{
    gl_FragColor = vec4(0.0, 0.5, 1.0, 1.0);
}";

static VERTEX_SHADER: &'static str = "
#version 300 es
in highp    vec4        position;
in highp    vec3        normal;
in highp    vec2        uv;

uniform     mat4        pvm;
uniform     mat4        view_model;

out         highp   vec3        v_normal;
out         highp   vec2        v_uv;
out         highp   vec3        v_orig_normal;
out         highp   vec3        v_light_dir;

void main() {
    gl_Position = pvm * vec4(position.xyz, 1.0);
    v_normal    = normalize((view_model * vec4(normal, 0.0)).xyz);
    v_light_dir = normalize((pvm * vec4(1.0, 0.0, 0.0, 1.0))).xyz;
    v_orig_normal   = normal;
    v_uv        = uv;
}";

static PIXEL_SHADER: &'static str = "
#version 300 es
precision mediump float;

        in highp    vec3        v_normal;
        in highp    vec3        v_orig_normal;
        in highp    vec3        v_light_dir;
        in highp    vec2        v_uv;

uniform highp       sampler2D   u_texture;

layout(location = 0) out lowp  vec4     color_buffer;
layout(location = 1) out highp vec4     normal_buffer;

void main() {
    highp float intensity = dot(v_light_dir, v_normal);
    highp vec4 col  = vec4(v_orig_normal.xyz, 1.0) * 0.9 + vec4(v_uv.xy, 0.0, 1.0) * 0.1;
    lowp vec4 t     = texture(u_texture, v_uv);
    color_buffer    = col * intensity;//uvec4(t * 255.0) + uvec4(vec4(v_normal.xyz, 0.0) * 255.0);
    normal_buffer   = col / 255.0 + t;
    color_buffer.a  = 1.0;
}";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ModelUniforms {
    pvm: Mat4f,
    view_model: Mat4f,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SolidModelUniforms {
    pvm: Mat4f,
}

pub struct PolyMeshRenderer {
    vb: NativeBuffer,
    ib: NativeBuffer,

    model_program: NativeProgram,
    solid_program: NativeProgram,

    max_vertex_count: usize,
    max_index_count: usize,

    model_attribs: HashMap<String, u32>,
    model_uniforms: HashMap<String, NativeUniformLocation>,

    solid_attribs: HashMap<String, u32>,
    solid_uniforms: HashMap<String, NativeUniformLocation>,

    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

unsafe impl Sync for PolyMeshRenderer {}
unsafe impl Send for PolyMeshRenderer {}

impl PolyMeshRenderer {
    pub fn create(driver: &glow::Context, max_tri_count: usize) -> Self {
        let max_vertex_count = max_tri_count * 3;
        let max_index_count = max_tri_count * 6;
        let vb = unsafe {
            let buff = driver.create_buffer().unwrap();
            let size = max_vertex_count * std::mem::size_of::<Vertex>();
            driver.bind_buffer(ARRAY_BUFFER, Some(buff));
            debug_assert!(driver.get_error() == 0);
            driver.buffer_data_size(ARRAY_BUFFER, size as _, glow::DYNAMIC_DRAW);
            debug_assert!(driver.get_error() == 0);
            buff
        };

        let ib = unsafe {
            let buff = driver.create_buffer().unwrap();
            let size = max_index_count * std::mem::size_of::<u32>();
            driver.bind_buffer(ELEMENT_ARRAY_BUFFER, Some(buff));
            debug_assert!(driver.get_error() == 0);
            driver.buffer_data_size(ELEMENT_ARRAY_BUFFER, size as _, glow::DYNAMIC_DRAW);
            debug_assert!(driver.get_error() == 0);
            buff
        };

        let model_program = create_program(driver, VERTEX_SHADER, PIXEL_SHADER).unwrap();
        let model_attribs = get_active_program_attributes(driver, model_program)
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i as u32))
            .collect::<HashMap<_, _>>();
        let model_uniforms = get_active_program_uniforms(driver, model_program)
            .iter()
            .map(|e| (e.name.clone(), unsafe { driver.get_uniform_location(model_program, &e.name).unwrap() }))
            .collect::<HashMap<_, _>>();

        let solid_program = create_program(driver, SOLID_VERTEX_SHADER, SOLID_PIXEL_SHADER).unwrap();
        let solid_attribs = get_active_program_attributes(driver, solid_program)
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i as u32))
            .collect::<HashMap<_, _>>();
        let solid_uniforms = get_active_program_uniforms(driver, solid_program)
            .iter()
            .map(|e| (e.name.clone(), unsafe { driver.get_uniform_location(solid_program, &e.name).unwrap() }))
            .collect::<HashMap<_, _>>();

        Self {
            vb,
            ib,

            model_program,
            solid_program,

            max_vertex_count,
            max_index_count,

            model_attribs,
            model_uniforms,

            solid_attribs,
            solid_uniforms,

            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn render_wire(&mut self, gl: &glow::Context, pvm: &Mat4f, _view_model: &Mat4f, pmesh: &PolyMesh) {
        self.vertices.clear();
        self.indices.clear();

        let pos_attr = self.solid_attribs["position"];

        unsafe {
            gl.disable(glow::BLEND);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::CULL_FACE);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::DEPTH_TEST);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::SCISSOR_TEST);
            debug_assert!(gl.get_error() == 0);

            gl.use_program(Some(self.solid_program));

            gl.enable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);

            let tm_ptr = pvm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&self.solid_uniforms["pvm"]), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vb));
            debug_assert!(gl.get_error() == 0);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ib));
            debug_assert!(gl.get_error() == 0);
        }

        for p in pmesh.polys() {
            let vid = self.vertices.len() as u32;
            let vertex_count = p.vertex_count();
            if self.indices.len() + (vertex_count - 2) * 2 > self.max_index_count || self.vertices.len() + vertex_count > self.max_vertex_count {
                unsafe {
                    // update the vertex buffer
                    let vertices_u8: &[u8] =
                        core::slice::from_raw_parts(self.vertices.as_ptr() as *const u8, self.vertices.len() * core::mem::size_of::<Vertex>());
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    // update the index buffer
                    let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 0);
                    debug_assert!(gl.get_error() == 0);

                    gl.draw_elements(glow::LINES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                    debug_assert!(gl.get_error() == 0);
                }

                self.vertices.clear();
                self.indices.clear();
            }

            for v in p {
                let position = pmesh.v_positions[v.pos].position;
                self.vertices.push(Vertex {
                    position,
                    normal: Vec3f::zero(),
                    uv: Vec2f::zero(),
                });
            }

            for i in 0..vertex_count as u32 {
                self.indices.push(vid + i);
                self.indices.push(vid + ((i + 1) % (vertex_count as u32)));
            }
        }

        if self.indices.len() > 0 {
            unsafe {
                // update the vertex buffer
                let vertices_u8: &[u8] = core::slice::from_raw_parts(self.vertices.as_ptr() as *const u8, self.vertices.len() * core::mem::size_of::<Vertex>());
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);
                gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 0);
                debug_assert!(gl.get_error() == 0);

                // update the index buffer
                let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);

                gl.draw_elements(glow::LINES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                debug_assert!(gl.get_error() == 0);
            }
        }

        unsafe {
            gl.disable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);
        }
    }

    pub fn render(&mut self, gl: &glow::Context, pvm: &Mat4f, view_model: &Mat4f, pmesh: &PolyMesh) {
        self.vertices.clear();
        self.indices.clear();

        let pos_attr = self.model_attribs["position"];
        let norm_attr = self.model_attribs["normal"];
        let uv_attr = self.model_attribs["uv"];

        unsafe {
            gl.disable(glow::BLEND);
            debug_assert!(gl.get_error() == 0);
            gl.disable(glow::CULL_FACE);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::DEPTH_TEST);
            debug_assert!(gl.get_error() == 0);
            gl.enable(glow::SCISSOR_TEST);
            debug_assert!(gl.get_error() == 0);

            gl.use_program(Some(self.model_program));

            gl.enable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);
            gl.enable_vertex_attrib_array(norm_attr);
            debug_assert!(gl.get_error() == 0);
            gl.enable_vertex_attrib_array(uv_attr);
            debug_assert!(gl.get_error() == 0);

            let tm_ptr = pvm.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&self.model_uniforms["pvm"]), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            let tm_ptr = view_model.col.as_ptr() as *const _ as *const f32;
            let slice = std::slice::from_raw_parts(tm_ptr, 16);
            gl.uniform_matrix_4_f32_slice(Some(&self.model_uniforms["view_model"]), false, &slice);
            debug_assert_eq!(gl.get_error(), 0);

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vb));
            debug_assert!(gl.get_error() == 0);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ib));
            debug_assert!(gl.get_error() == 0);
        }

        for p in pmesh.polys() {
            let vid = self.vertices.len() as u32;
            let vertex_count = p.vertex_count();
            if self.indices.len() + (vertex_count - 2) * 2 > self.max_index_count || self.vertices.len() + vertex_count > self.max_vertex_count {
                unsafe {
                    // update the vertex buffer
                    let vertices_u8: &[u8] =
                        core::slice::from_raw_parts(self.vertices.as_ptr() as *const u8, self.vertices.len() * core::mem::size_of::<Vertex>());
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    // update the index buffer
                    let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 0);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(norm_attr, 3, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 12);
                    debug_assert!(gl.get_error() == 0);

                    gl.vertex_attrib_pointer_f32(uv_attr, 2, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 24);
                    debug_assert!(gl.get_error() == 0);

                    gl.draw_elements(glow::TRIANGLES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                    debug_assert!(gl.get_error() == 0);
                }

                self.vertices.clear();
                self.indices.clear();
            }

            for v in p {
                let position = pmesh.v_positions[v.pos].position;
                let normal = pmesh.v_normals[v.normal];
                let uv = if pmesh.v_tex.len() > 0 { pmesh.v_tex[v.tex] } else { Vec2f::zero() };
                self.vertices.push(Vertex { position, normal, uv });
            }

            for i in 2..vertex_count as u32 {
                self.indices.push(vid);
                self.indices.push(vid + i - 1);
                self.indices.push(vid + i);
            }
        }

        if self.indices.len() > 0 {
            unsafe {
                // update the vertex buffer
                let vertices_u8: &[u8] = core::slice::from_raw_parts(self.vertices.as_ptr() as *const u8, self.vertices.len() * core::mem::size_of::<Vertex>());
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);

                // update the index buffer
                let indices_u8: &[u8] = core::slice::from_raw_parts(self.indices.as_ptr() as *const u8, self.indices.len() * core::mem::size_of::<u32>());
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices_u8, glow::DYNAMIC_DRAW);
                debug_assert!(gl.get_error() == 0);

                gl.vertex_attrib_pointer_f32(pos_attr, 3, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 0);
                debug_assert!(gl.get_error() == 0);

                gl.vertex_attrib_pointer_f32(norm_attr, 3, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 12);
                debug_assert!(gl.get_error() == 0);

                gl.vertex_attrib_pointer_f32(uv_attr, 2, glow::FLOAT, false, core::mem::size_of::<Vertex>() as i32, 24);
                debug_assert!(gl.get_error() == 0);

                gl.draw_elements(glow::TRIANGLES, self.indices.len() as _, glow::UNSIGNED_INT, 0);
                debug_assert!(gl.get_error() == 0);
            }
        }

        unsafe {
            gl.disable_vertex_attrib_array(pos_attr);
            debug_assert!(gl.get_error() == 0);
            gl.disable_vertex_attrib_array(norm_attr);
            debug_assert!(gl.get_error() == 0);
            gl.disable_vertex_attrib_array(uv_attr);
            debug_assert!(gl.get_error() == 0);
            gl.use_program(None);
            debug_assert!(gl.get_error() == 0);
        }
    }
}
