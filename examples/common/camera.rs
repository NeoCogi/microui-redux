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
use rs_math3d::*;

const TRACKBALL_SIZE: f32 = 0.8;
const EPSILON: f32 = 1.0 / (1024.0 * 1024.0);

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Camera {
    target: Vec3f,
    distance: f32,
    rotation: Quatf,

    pos: Vec3f,
    up: Vec3f,
    direction: Vec3f,

    view: Mat4f,
    projection: Mat4f,

    fov: f32,
    aspect: f32,
    near_plane: f32,
    far_plane: f32,
}

impl Camera {
    fn project_to_track_ball(pt: &Vec2f, r: f32) -> f32 {
        let d = pt.length();
        if d < r * 0.70710678118654752440 {
            // inside the sphere
            (r * r - d * d).sqrt()
        } else {
            // project on a hyperbola
            let t = r / 1.41421356237309504880;
            t * t / d
        }
    }

    pub fn tracball_rotate(&self, viewport: Dimensioni, from: &Vec2f, to: &Vec2f) -> Self {
        if (*to - *from).length().abs() == 0.0 {
            return *self;
        }

        let aspect = (viewport.width as f32) / (viewport.height as f32);

        let start = from;
        let end = to; // do we need to divide by aspect ?

        if start.length() > TRACKBALL_SIZE || end.length() > TRACKBALL_SIZE {
            return *self;
        }

        let zs = Self::project_to_track_ball(&start, TRACKBALL_SIZE);
        let ze = Self::project_to_track_ball(&end, TRACKBALL_SIZE);

        let start_axis = Vec3f::normalize(&Vec3f::new(start.x, start.y, zs));
        let end_axis = Vec3f::normalize(&Vec3f::new(end.x, end.y, ze));

        // compute rotation axis
        let rot_axis = -Vec3f::cross(&start_axis, &end_axis);
        let rot_axis_len = Vec3f::length(&rot_axis);

        if rot_axis_len < EPSILON {
            return *self;
        }

        let t = Vec3f::dot(&start_axis, &end_axis);
        let n = Quatf::normalize(&Quatf::new(rot_axis.x, rot_axis.y, rot_axis.z, 1.0 + t));
        Self::new(
            self.target,
            self.distance,
            Quatf::normalize(&(self.rotation * n)),
            self.fov,
            aspect,
            self.near_plane,
            self.far_plane,
        )
    }

    fn unproject(pvm: &Mat4f, pt: &Vec3f) -> Vec3f {
        let lb = Vec2f::new(-1.0, -1.0);
        let tr = Vec2f::new(1.0, 1.0);
        unproject3(&Mat4f::identity(), &pvm, &lb, &tr, pt)
    }

    pub fn pan(&self, viewport: Dimensioni, from: &Vec2f, to: &Vec2f) -> Self {
        if Vec2f::length(&(*to - *from)).abs() == 0.0 {
            return *self;
        }

        let aspect = (viewport.width as f32) / (viewport.height as f32);

        let pv_mat = self.projection * self.view;

        //
        // these 2 are always going to give the normal as (0, 0, 1)
        // Keep them for informative reason
        //
        let near_center_proj = Vec3f::new(0.0, 0.0, -1.0);
        let near_center = Self::unproject(&pv_mat, &near_center_proj);

        let far_center_proj = Vec3f::new(0.0, 0.0, 1.0);
        let far_center = Self::unproject(&pv_mat, &far_center_proj);

        let normal = Vec3f::normalize(&(near_center - far_center));
        let p = Planef::new(&normal, &self.target);

        let near_prev_proj = Vec3f::new(from.x, from.y, -1.0);
        let near_prev = Self::unproject(&pv_mat, &near_prev_proj);
        let far_prev_proj = Vec3f::new(from.x, from.y, 1.0);
        let far_prev = Self::unproject(&pv_mat, &far_prev_proj);
        let prev_center = p.intersect_ray(&Ray3f::new(&near_prev, &(far_prev - near_prev)));

        let near_curr_proj = Vec3f::new(to.x, to.y, -1.0);
        let near_curr = Self::unproject(&pv_mat, &near_curr_proj);
        let far_curr_proj = Vec3f::new(to.x, to.y, 1.0);
        let far_curr = Self::unproject(&pv_mat, &far_curr_proj);
        let curr_center = p.intersect_ray(&Ray3f::new(&near_curr, &(far_curr - near_curr)));

        match (prev_center, curr_center) {
            (Some(p), Some(c)) => Self::new(
                self.target + (p - c),
                self.distance,
                self.rotation,
                self.fov,
                aspect,
                self.near_plane,
                self.far_plane,
            ),
            _ => *self,
        }
    }

    pub fn new(
        target: Vec3f,
        distance: f32,
        rotation: Quatf,
        fov: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let pos = Vec3f::new(0.0, 0.0, 1.0);
        let up = Vec3f::new(0.0, 1.0, 0.0);

        let rot_matrix = Quatf::mat4(&rotation);
        let cam_pos = target + transform_vec3(&rot_matrix, &pos) * distance;
        let cam_up = transform_vec3(&rot_matrix, &up);
        let cam_dir = Vec3f::normalize(&(target - cam_pos));

        let view = lookat(&cam_pos, &target, &cam_up);
        let projection = perspective(fov, aspect, near, far);

        Self {
            target: target,
            distance: distance,
            rotation: rotation,

            pos: cam_pos,
            up: cam_up,
            direction: cam_dir,

            view: view,
            projection: projection,

            fov: fov,
            aspect: aspect,
            near_plane: near,
            far_plane: far,
        }
    }

    fn update_matrices(&mut self) {
        let pos = Vec3f::new(0.0, 0.0, 1.0);
        let up = Vec3f::new(0.0, 1.0, 0.0);

        let rot_matrix = Quatf::mat4(&self.rotation);
        let cam_pos = self.target + transform_vec3(&rot_matrix, &pos) * self.distance;
        let cam_up = transform_vec3(&rot_matrix, &up);
        //let cam_dir     = Vec3f::normalize(&(self.target - cam_pos));

        self.view = lookat(&cam_pos, &self.target, &cam_up);
        self.projection = perspective(self.fov, self.aspect, self.near_plane, self.far_plane);
    }

    pub fn position(&self) -> Vec3f {
        self.pos
    }
    pub fn rotation(&self) -> Quatf {
        self.rotation
    }
    pub fn up(&self) -> Vec3f {
        self.up
    }
    pub fn direction(&self) -> Vec3f {
        self.direction
    }
    pub fn distance(&self) -> f32 {
        self.distance
    }
    pub fn target(&self) -> Vec3f {
        self.target
    }
    pub fn view_matrix(&self) -> &Mat4f {
        &self.view
    }
    pub fn projection_matrix(&self) -> &Mat4f {
        &self.projection
    }
    pub fn with_aspect(mut self, aspect: f32) -> Self {
        self.aspect = aspect;
        self.update_matrices();
        self
    }
    pub fn fov(&self) -> f32 {
        self.fov
    }
    pub fn with_fov(mut self, fov: f32) -> Self {
        self.fov = fov;
        self.projection = perspective(self.fov, self.aspect, self.near_plane, self.far_plane);
        self
    }

    pub fn near_plane(&self) -> f32 {
        self.near_plane
    }
    pub fn with_near_plane(mut self, np: f32) -> Self {
        self.near_plane = np;
        self.update_matrices();
        self
    }

    pub fn far_plane(&self) -> f32 {
        self.far_plane
    }
    pub fn with_far_plane(mut self, fp: f32) -> Self {
        self.far_plane = fp;
        self.update_matrices();
        self
    }
}
