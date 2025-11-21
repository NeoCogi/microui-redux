#![allow(dead_code)]
//
// Copyright 2021-Present (c) Raja Lehtihet & Wael El Oraiby
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
use super::*;
use camera::*;

pub enum UpdateResult {
    Handled,
    Unhandled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavigationMode {
    Pan,
    Orbit,
}

pub struct View3D {
    camera: Camera,
    nav_mode: NavigationMode,
    dimension: Dimensioni,
    scroll: f32,
    bounds: Box3f,
    pvm: Mat4f,
}

impl View3D {
    pub fn new(camera: Camera, dimension: Dimensioni, bounds: Box3f) -> Self {
        let scroll = camera.distance();
        Self {
            camera,
            nav_mode: NavigationMode::Orbit,
            dimension,
            scroll,
            bounds,
            pvm: camera.projection_matrix().clone() * camera.view_matrix().clone(),
        }
    }

    fn normalize_pointer_pos(&self, pos: &Vec2i) -> Vec2f {
        let dim_f = Vec2f::new(self.dimension.width as _, self.dimension.height as _);
        let pos_f = Vec2f::new(pos.x as _, pos.y as _);

        let mut norm_pos = 2.0 * pos_f / dim_f - Vec2f::new(1.0, 1.0);

        norm_pos.y *= -1.0;
        norm_pos
    }

    pub fn update(&mut self, event: MouseEvent) -> UpdateResult {
        // TODO: do proper computation of the far plane
        let far_plane = self.bounds.extent().length() * 100.0;
        self.camera = self.camera.with_far_plane(far_plane);

        let handled = match (&self.nav_mode, event) {
            (NavigationMode::Orbit, MouseEvent::Drag { prev_pos: prev, curr_pos: curr }) => {
                let p = self.normalize_pointer_pos(&prev);
                let c = self.normalize_pointer_pos(&curr);

                self.camera = self.camera.tracball_rotate(self.dimension, &p, &c);
                UpdateResult::Handled
            }

            (NavigationMode::Pan, MouseEvent::Drag { prev_pos: prev, curr_pos: curr }) => {
                let p = Vec2f::new(prev.x as _, prev.y as _);
                let c = Vec2f::new(curr.x as _, curr.y as _);
                self.camera = self.camera.pan(self.dimension, &p, &c);
                UpdateResult::Handled
            }

            (_, MouseEvent::Scroll(v)) => {
                self.scroll += v;
                self.scroll = f32::max(0.5, self.scroll);
                let distance = self.scroll;
                let aspect = (self.dimension.width as f32) / (self.dimension.height as f32);
                self.camera = Camera::new(
                    self.camera.target(),
                    distance,
                    self.camera.rotation(),
                    self.camera.fov(),
                    aspect,
                    self.camera.near_plane(),
                    self.camera.far_plane(),
                );
                UpdateResult::Handled
            }

            _ => UpdateResult::Unhandled,
        };

        self.pvm = self.camera.projection_matrix().clone() * self.camera.view_matrix().clone();
        handled
    }

    pub fn set_dimension(&mut self, dimension: Dimensioni) {
        self.dimension = dimension;
        let aspect = (self.dimension.width as f32) / (self.dimension.height as f32);
        self.camera = self.camera.with_aspect(aspect);
        self.update(MouseEvent::None);
    }

    pub fn get_navigation_mode(&self) -> NavigationMode { self.nav_mode }
    pub fn set_navigation_mode(&mut self, nav_mode: NavigationMode) {
        if nav_mode != self.nav_mode {
            self.nav_mode = nav_mode;
            self.update(MouseEvent::None); // idem potent in this case
        }
    }

    pub fn pvm(&self) -> Mat4f { self.pvm }

    pub fn projection_matrix(&self) -> Mat4f { self.camera.projection_matrix().clone() }

    pub fn view_matrix(&self) -> Mat4f { self.camera.view_matrix().clone() }
}
