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
use super::*;

#[derive(Clone)]
struct PoolItem<ID, PO: Clone> {
    object: PO,
    frame: usize,
    children: Vec<ID>,
}

#[derive(Clone)]
pub struct Pool<ID, PO: Clone> {
    items: HashMap<ID, PoolItem<ID, PO>>,
    stack: Vec<Id>,
    gc_ids: Vec<ID>,
}

impl<ID: PartialEq + Eq + Hash + Clone + core::fmt::Debug, PO: Clone> Pool<ID, PO> {
    pub fn insert(&mut self, id: ID, object: PO, frame: usize) -> ID {
        match self.items.get_mut(&id) {
            Some(v) => v.frame = frame,
            None => {
                println!("object :{:?} created", id);
                self.items.insert(id.clone(), PoolItem { object, frame, children: Vec::new() });
            }
        }
        id
    }

    pub fn get(&self, id: ID) -> Option<&PO> {
        self.items.get(&id).map(|pi| &pi.object)
    }

    pub fn get_mut(&mut self, id: ID) -> Option<&mut PO> {
        self.items.get_mut(&id).map(|po| &mut po.object)
    }

    pub fn update(&mut self, id: ID, frame: usize) {
        self.items.get_mut(&id).unwrap().frame = frame
    }

    pub fn remove(&mut self, id: ID) {
        self.items.remove(&id);
    }

    pub fn gc(&mut self, current_frame: usize) {
        self.gc_ids.clear();
        for kv in &self.items {
            if kv.1.frame < current_frame - 2 {
                self.gc_ids.push(kv.0.clone());
            }
        }

        for gid in &self.gc_ids {
            self.items.remove(gid);
        }
    }
}

impl<ID, PO: Clone> Default for Pool<ID, PO> {
    fn default() -> Self {
        Self {
            items: HashMap::default(),
            stack: Vec::new(),
            gc_ids: Vec::default(),
        }
    }
}
