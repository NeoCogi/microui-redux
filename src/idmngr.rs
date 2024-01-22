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

#[derive(Default, Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Id(u32);

pub struct IdManager {
    last_id: Option<Id>,
    id_stack: Vec<Id>,
}

impl IdManager {
    pub fn new() -> Self {
        Self { last_id: None, id_stack: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.id_stack.len()
    }

    pub fn last_id(&self) -> Option<Id> {
        self.last_id
    }

    pub fn push_id(&mut self, id: Id) {
        self.id_stack.push(id)
    }

    fn hash_step(h: u32, n: u32) -> u32 {
        (h ^ n).wrapping_mul(16777619 as u32)
    }

    fn hash_u32(hash_0: &mut Id, orig_id: u32) {
        let bytes = orig_id.to_be_bytes();
        for b in bytes {
            *hash_0 = Id(Self::hash_step(hash_0.0, b as u32));
        }
    }

    fn hash_str(hash_0: &mut Id, s: &str) {
        for c in s.chars() {
            *hash_0 = Id(Self::hash_step(hash_0.0, c as u32));
        }
    }

    fn hash_bytes(hash_0: &mut Id, s: &[u8]) {
        for c in s {
            *hash_0 = Id(Self::hash_step(hash_0.0, *c as u32));
        }
    }
    pub fn get_id_u32(&mut self, orig_id: u32) -> Id {
        let mut res: Id = match self.id_stack.last() {
            Some(id) => *id,
            None => Id(2166136261),
        };
        Self::hash_u32(&mut res, orig_id);
        self.last_id = Some(res);
        return res;
    }

    pub fn get_id_from_ptr<T: ?Sized>(&mut self, orig_id: &T) -> Id {
        let mut res: Id = match self.id_stack.last() {
            Some(id) => *id,
            None => Id(2166136261),
        };
        let ptr = orig_id as *const T as *const u8 as usize;
        let bytes = ptr.to_le_bytes();
        Self::hash_bytes(&mut res, &bytes);
        self.last_id = Some(res);
        return res;
    }

    pub fn get_id_from_str(&mut self, s: &str) -> Id {
        let mut res: Id = match self.id_stack.last() {
            Some(id) => *id,
            None => Id(2166136261),
        };
        Self::hash_str(&mut res, s);
        self.last_id = Some(res);
        return res;
    }

    pub fn push_id_from_ptr<T>(&mut self, orig_id: &T) {
        let id = self.get_id_from_ptr(orig_id);
        self.id_stack.push(id);
    }

    pub fn push_id_from_str(&mut self, s: &str) {
        let id = self.get_id_from_str(s);
        self.id_stack.push(id);
    }

    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }
}
