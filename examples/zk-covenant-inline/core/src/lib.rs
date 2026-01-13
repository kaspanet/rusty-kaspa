#![no_std]

extern crate alloc;
extern crate core;

use sha2::Digest;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Fib(u8) = 0,
    Factorial(u8) = 1,
}

impl Action {
    pub fn split(self) -> (u8, u8) {
        match self {
            Action::Fib(a) => (0, a),
            Action::Factorial(a) => (1, a),
        }
    }
}

impl TryFrom<[u8; 2]> for Action {
    type Error = &'static str;

    fn try_from([discriminator, value]: [u8; 2]) -> Result<Self, Self::Error> {
        match discriminator {
            0 => Ok(Action::Fib(value)),
            1 => Ok(Action::Factorial(value)),
            _ => Err("Invalid discriminator"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct VersionedActionRaw {
    pub action_version: u16,
    pub action_raw: [u8; 2],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct PublicInput {
    pub versioned_action_raw: VersionedActionRaw,
    pub prev_state_hash: [u32; 8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ActionWithOutput {
    pub action: u8,
    pub input: u8,
    _reserved: [u8; 2],
    pub output: u32,
}

impl ActionWithOutput {
    pub fn new(action: Action, output: u32) -> Self {
        let (action, input) = action.split();
        Self { action, input, _reserved: [0; _], output }
    }

    pub fn as_word_slice(&self) -> &[u32] {
        bytemuck::cast_slice(core::slice::from_ref(self))
    }
    pub fn as_half_word_slice(&self) -> &[u16] {
        bytemuck::cast_slice(core::slice::from_ref(self))
    }
}

pub const VERSION: u16 = 0;
const N: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct State {
    version: u16,
    current: i16,
    results_ring: [ActionWithOutput; N],
}

impl State {
    pub fn current(&self) -> i16 {
        self.current
    }
    pub fn add_new_result(&mut self, action: Action, output: u32) {
        self.current = (self.current + 1) % N as i16;
        self.results_ring[self.current as usize] = ActionWithOutput::new(action, output);
    }
    pub fn get_result(&self, index: usize) -> Option<&ActionWithOutput> {
        assert!(index < N);
        if index > self.current as usize {
            None
        } else {
            self.results_ring.get(index % N)
        }
    }

    pub fn last_result(&self) -> Option<&ActionWithOutput> {
        self.get_result(self.current as usize)
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    pub fn as_word_slice(&self) -> &[u32] {
        bytemuck::cast_slice(core::slice::from_ref(self))
    }

    pub fn as_half_word_slice(&self) -> &[u16] {
        bytemuck::cast_slice(core::slice::from_ref(self))
    }

    pub fn hash(&self) -> [u32; 8] {
        const DOMAIN: &[u8; 4] = b"STAT";
        let mut hasher = sha2::Sha256::new_with_prefix(DOMAIN);
        hasher.update(self.as_bytes());
        let mut out = [0u32; 8];
        let d = sha2::Digest::finalize(hasher);
        bytemuck::bytes_of_mut(&mut out).copy_from_slice(d.as_slice());
        out
    }
}

impl Default for State {
    fn default() -> Self {
        Self { results_ring: [ActionWithOutput::default(); _], current: -1, version: VERSION }
    }
}
