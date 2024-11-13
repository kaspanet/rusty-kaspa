use kaspa_rpc_core::api::ops::RpcApiOps;

// Struct to manage flags as a combined bitmask
#[derive(Debug)]
pub struct Flags {
    bitmask: u128,
}

impl Flags {
    // Create an empty flag set
    pub fn new() -> Self {
        Flags { bitmask: 0 }
    }

    // Adds a flag
    pub fn add(&mut self, op: RpcApiOps) {
        self.bitmask |= op.bitmask();
    }

    // Removes a flag
    pub fn remove(&mut self, op: RpcApiOps) {
        self.bitmask &= !op.bitmask();
    }

    // Check if a flag is enabled
    pub fn has_enabled(&self, op: RpcApiOps) -> bool {
        (self.bitmask & op.bitmask()) != 0
    }

    // Create a flag set from a slice of operations
    pub fn from_ops(ops: &[RpcApiOps]) -> Self {
        let mut permissions = Flags::new();
        for &op in ops {
            permissions.add(op);
        }
        permissions
    }
}

impl From<u128> for Flags {
    fn from(bitmask: u128) -> Self {
        Flags { bitmask }
    }
}
