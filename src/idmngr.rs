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
