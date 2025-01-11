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
use std::io;

use glow::*;

pub fn create_program(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<NativeProgram, io::Error> {
    unsafe {
        let program = gl.create_program().expect("Cannot create program");

        let shader_sources = [(glow::VERTEX_SHADER, vertex_shader_source), (glow::FRAGMENT_SHADER, fragment_shader_source)];

        let mut shaders = Vec::with_capacity(shader_sources.len());

        for (shader_type, shader_source) in shader_sources.iter() {
            let shader = gl.create_shader(*shader_type).expect("Cannot create shader");
            gl.shader_source(shader, shader_source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let error_string = format!("{}", gl.get_shader_info_log(shader));
                for shader in shaders {
                    gl.delete_shader(shader);
                }
                gl.delete_program(program);
                return Err(io::Error::new(io::ErrorKind::Other, error_string));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }

        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let error_string = format!("{}", gl.get_program_info_log(program));
            for shader in shaders {
                gl.delete_shader(shader);
            }
            gl.delete_program(program);
            return Err(io::Error::new(io::ErrorKind::Other, error_string));
        }

        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }

        Ok(program)
    }
}

pub fn get_active_program_attributes(gl: &glow::Context, program: NativeProgram) -> Vec<ActiveAttribute> {
    let mut attribs = Vec::new();
    unsafe {
        let attrib_count = gl.get_active_attributes(program);
        for index in 0..attrib_count {
            let attr = gl.get_active_attribute(program, index);
            match attr {
                Some(attr) => attribs.push(attr),
                _ => (),
            }
        }
    }
    attribs
}

pub fn get_active_program_uniforms(gl: &glow::Context, program: NativeProgram) -> Vec<ActiveUniform> {
    let mut unis = Vec::new();
    unsafe {
        let attrib_count = gl.get_active_uniforms(program);
        for index in 0..attrib_count {
            let uni = gl.get_active_uniform(program, index);
            match uni {
                Some(uni) => unis.push(uni),
                _ => (),
            }
        }
    }
    unis
}
