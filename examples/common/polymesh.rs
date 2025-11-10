#![allow(dead_code)]
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
use super::*;
use glow_renderer::PolymeshTrait;

use std::iter::*;

#[repr(C)]
pub struct Vertex {
    pub position: Vec3f,
    pub normal: Vec3f,
    pub uv: Vec2f,
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

impl glow_renderer::PolymeshVertex for &PolyVertex {
    fn pos(&self) -> usize {
        self.pos
    }
    fn normal(&self) -> usize {
        self.normal
    }
    fn tex(&self) -> usize {
        self.tex
    }
}

impl<'a> glow_renderer::PolymeshPolygon for PolygonIterator<'a> {
    fn vertex_count(&self) -> usize {
        self.mesh.polys[self.poly_id].len
    }
}

impl<'a> PolymeshTrait for &'a PolyMesh {
    type PolyIter = PolyMeshIterator<'a>;
    type VertexIter = PolygonIterator<'a>;
    type Vertex = &'a PolyVertex;

    fn polys(&self) -> Self::PolyIter {
        PolyMeshIterator { mesh: *self, poly_id: 0 }
    }

    fn get_vertex_position(&self, index: usize) -> Vec3f {
        self.v_positions[index].position
    }

    fn get_vertex_normal(&self, index: usize) -> Vec3f {
        self.v_normals[index]
    }

    fn get_vertex_uv(&self, index: usize) -> Vec2f {
        if self.v_tex.len() > index {
            self.v_tex[index]
        } else {
            Vec2f::zero()
        }
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
