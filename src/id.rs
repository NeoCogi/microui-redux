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
//! Stable numeric identifiers shared across windows, widgets, and retained nodes.

use std::hash::{Hash, Hasher};

/// FNV-1a offset basis used by stable internal id hashing.
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a prime used by stable internal id hashing.
const FNV_PRIME: u64 = 0x100000001b3;
/// Global salt mixed into scoped framework-generated ids.
const MICROUI_ID_SALT: u64 = 0x6d69_6372_6f75_695f;

/// Numeric identifier value.
#[derive(Default, Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Id(usize);

impl Id {
    /// Creates an ID from the address of a stable object.
    pub fn from_ptr<T: ?Sized>(value: &T) -> Self {
        Self(value as *const T as *const () as usize)
    }

    /// Creates a caller-supplied numeric value.
    /// On 32-bit platforms the value is truncated to fit in a `usize`.
    pub fn new(value: u64) -> Self {
        Self(value as usize)
    }

    /// Creates a stable ID from a string label using FNV-1a hashing.
    pub fn from_str(label: &str) -> Self {
        let mut hash = IdHasher::new();
        hash.write(label.as_bytes());
        hash.into_id()
    }

    /// Returns the raw numeric value wrapped by this ID.
    pub fn raw(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug)]
/// Deterministic FNV-1a hasher used for UI ids.
pub(crate) struct IdHasher {
    /// Current hash accumulator.
    hash: u64,
}

impl Default for IdHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl IdHasher {
    /// Creates a hasher initialized to the FNV-1a offset basis.
    pub(crate) const fn new() -> Self {
        Self { hash: FNV_OFFSET_BASIS }
    }

    /// Converts the current hash accumulator into an [`Id`].
    pub(crate) fn into_id(self) -> Id {
        Id::new(self.hash)
    }
}

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= *byte as u64;
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes());
    }

    fn write_i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn write_i128(&mut self, value: i128) {
        self.write(&value.to_le_bytes());
    }

    fn write_isize(&mut self, value: isize) {
        self.write_i64(value as i64);
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Salted namespace for framework-generated ids.
pub(crate) struct IdNamespace {
    /// Namespace-specific salt mixed into every id generated through this value.
    salt: u64,
}

impl IdNamespace {
    /// Namespace used for window chrome controls.
    pub(crate) const WINDOW_CHROME: Self = Self::new(0x726f_6f74);
    /// Namespace used for nested scroll-area scopes.
    pub(crate) const SCROLL_AREA_SCOPE: Self = Self::new(0x7061_6e65_6c5f_7363);
    /// Namespace used for internal widgets inside composite controls.
    pub(crate) const INTERNAL_CONTROL: Self = Self::new(0x696e_7465_726e_616c);
    /// Namespace used for auto-generated UI nodes.
    pub(crate) const UINODE_BUILDER: Self = Self::new(0x7769_6467_6574_7472);
    /// Namespace used for synthetic runtime root container nodes.
    pub(crate) const UINODE_ROOT: Self = Self::new(0x7569_6e6f_6465_726f);

    /// Creates a namespace from a caller-provided salt.
    const fn new(salt: u64) -> Self {
        Self { salt }
    }

    /// Hashes the provided values into a stable id within this namespace.
    pub(crate) fn id(self, values: impl IntoIterator<Item = u64>) -> Id {
        let mut hash = IdHasher::new();
        MICROUI_ID_SALT.hash(&mut hash);
        self.salt.hash(&mut hash);
        for value in values {
            value.hash(&mut hash);
        }
        hash.into_id()
    }
}

/// Hashes a stable key into a raw `u64` for retained id generation.
pub(crate) fn hash_id_key<K: Hash>(key: K) -> u64 {
    let mut hash = IdHasher::new();
    key.hash(&mut hash);
    hash.finish()
}
