//
// Copyright (C) 2020 Raja Lehtihet & Wael El Oraiby
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

use super::polymesh::*;
use rs_math3d::*;

use std::fs::*;
use std::io::Read;

pub enum ObjError {
    ParseFloatError,
    ExpectingVec2f,
    ExpectingVec3f,
    ExpectingVertexOrVertexUV,
    ExpectingTriangleOrQuad,
    ExpectingPart,
}

fn parse_vec3(parts: &[&str], verts: &mut Vec<Vec3f>) -> Result<i32, String> {
    if parts.len() != 3 {
        return Err(String::from("expecting 3 floats"));
    }

    let f0: Result<f32, _> = parts[0].parse();
    let f1: Result<f32, _> = parts[1].parse();
    let f2: Result<f32, _> = parts[2].parse();

    match (f0, f1, f2) {
        (Ok(f0), Ok(f1), Ok(f2)) => {
            verts.push(Vec3f::new(f0, f1, f2));
            Ok(0)
        }
        _ => Err(String::from("float parse error")),
    }
}

fn parse_vec2(parts: &[&str], uvws: &mut Vec<Vec2f>) -> Result<i32, String> {
    if parts.len() != 2 {
        return Err(String::from("expecting 2 floats"));
    }

    let f0: Result<f32, _> = parts[0].parse();
    let f1: Result<f32, _> = parts[1].parse();

    match (f0, f1) {
        (Ok(f0), Ok(f1)) => {
            uvws.push(Vec2f::new(f0, f1));
            Ok(0)
        }
        _ => Err(String::from("float parse error")),
    }
}

fn parse_face_part(part: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<&str> = part.split('/').collect();
    if parts.len() != 1 && parts.len() != 3 {
        return Result::Err(String::from("expecting vertex or vertex//uv"));
    }

    let v: Result<u32, _> = parts[0].parse();
    let n: Result<u32, _> = parts[2].parse();
    let uv: Result<u32, _> = if parts.len() == 1 { Ok(0) } else { parts[1].parse() };

    match (v, n, uv) {
        (Ok(v), Ok(n), Ok(uv)) => Result::Ok((v - 1, n - 1, uv - 1)),
        _ => Result::Err(String::from("expecting vertex/uv/normal, vertex/uv or vertex")),
    }
}

pub struct ObjFace {
    len: usize,
    vs_idx: usize,
}

fn parse_face(parts: &[&str], face_verts: &mut Vec<(u32, u32, u32)>, faces: &mut Vec<ObjFace>) -> Result<i32, String> {
    let of = ObjFace {
        len: parts.len(),
        vs_idx: face_verts.len(),
    };

    for p in parts {
        match parse_face_part(p) {
            Ok(vuv) => face_verts.push(vuv),
            Err(err) => return Result::Err(err),
        }
    }

    faces.push(of);
    Ok((faces.len() - 1) as _)
}

fn parse_line(
    line: &str,
    verts: &mut Vec<Vec3f>,
    normals: &mut Vec<Vec3f>,
    uvs: &mut Vec<Vec2f>,
    face_verts: &mut Vec<(u32, u32, u32)>,
    faces: &mut Vec<ObjFace>,
) -> Result<i32, String> {
    if line == "" {
        return Result::Ok(0);
    }

    let parts: Vec<&str> = line.split(|x| x == ' ' || x == '\t').filter(|&x| x != "").collect();
    if parts.len() == 0 {
        return Result::Err(String::from("No part!"));
    }

    if parts[0].starts_with('#') {
        return Result::Ok(1);
    }

    match parts[0] {
        "v" => parse_vec3(&parts[1..], verts),
        "vn" => parse_vec3(&parts[1..], normals),
        "vt" => parse_vec2(&parts[1..], uvs),
        "f" => parse_face(&parts[1..], face_verts, faces),
        _ => Result::Ok(2),
    }
}

pub struct Obj {
    verts: Vec<Vec3f>,
    normals: Vec<Vec3f>,
    uvs: Vec<Vec2f>,
    face_verts: Vec<(u32, u32, u32)>,
    faces: Vec<ObjFace>,
}

impl Obj {
    pub fn from_byte_stream(bs: &[u8]) -> Result<Obj, String> {
        let lines = std::str::from_utf8(bs).unwrap();

        let mut verts = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut face_verts = Vec::new();
        let mut faces = Vec::new();

        for l in lines.lines() {
            match parse_line(l, &mut verts, &mut normals, &mut uvs, &mut face_verts, &mut faces) {
                Ok(_) => (),
                Err(err) => return Err(err),
            }
        }

        Ok(Obj { verts, normals, uvs, face_verts, faces })
    }

    pub fn from_file(path: &str) -> Result<Obj, String> {
        let file = File::open(path);
        match file {
            Ok(_) => (),
            Err(_) => return Err(String::from("Could not open file")),
        }

        let mut f = file.unwrap();
        let mut lines = String::new();
        f.read_to_string(&mut lines).unwrap();

        Self::from_byte_stream(lines.as_str().as_bytes())
    }

    pub fn to_polymesh(&self) -> PolyMesh {
        let mut verts = Vec::new();
        let mut pm = PolyMesh::new();
        pm.set_vertices(self.verts.clone(), self.normals.clone(), self.uvs.clone());

        for (_, f) in self.faces.iter().enumerate() {
            for i in 0..f.len {
                let (v_id, n_id, uv_id) = self.face_verts[(f.vs_idx + i) as usize];
                verts.push(PolyVertex {
                    pos: v_id as _,
                    normal: n_id as _,
                    tex: uv_id as _,
                });
            }
            pm.add_poly(&verts);
            verts.clear();
        }

        pm
    }
}
