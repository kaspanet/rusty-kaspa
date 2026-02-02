#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Fib(u8) = 0,
    Factorial(u8) = 1,
}

impl Action {
    pub fn execute(self) -> u32 {
        match self {
            Action::Fib(n) => fib(n),
            Action::Factorial(n) => factorial(n),
        }
    }
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
#[repr(C, align(4))]
pub struct VersionedActionRaw {
    pub action_version: u16,
    pub action_raw: [u8; 2],
    pub nonce: u32,
}

impl VersionedActionRaw {
    pub fn as_words(&self) -> &[u32] {
        bytemuck::cast_slice(bytemuck::bytes_of(self))
    }

    pub fn as_words_mut(&mut self) -> &mut [u32] {
        bytemuck::cast_slice_mut(bytemuck::bytes_of_mut(self))
    }

    pub fn from_words(words: [u32; size_of::<Self>() / 4]) -> Self {
        bytemuck::cast(words)
    }
}

fn fib(n: u8) -> u32 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }
    let mut a = 0u32;
    let mut b = 1u32;
    for _ in 2..=n {
        let temp = a.saturating_add(b);
        a = b;
        b = temp;
    }
    b
}

fn factorial(n: u8) -> u32 {
    let mut res = 1u32;
    for i in 1..=n {
        res = res.saturating_mul(i as u32);
    }
    res
}
