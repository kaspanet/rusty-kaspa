
#[derive(Debug)]
pub enum Instruction {
    Shutdown,
    TestInstructionForService(u64),
    TestInstructionForConsumer(u64),
}

