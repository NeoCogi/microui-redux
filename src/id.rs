//! Stable numeric identifiers shared across windows, widgets, and retained trees.

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
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET_BASIS;
        for byte in label.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        Self::new(hash)
    }

    /// Returns the raw numeric value wrapped by this ID.
    pub fn raw(self) -> usize {
        self.0
    }
}
