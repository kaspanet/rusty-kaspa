pub trait StagingArea {
    fn commit(&mut self);
}

pub struct InMemoryStagingArea {}

impl StagingArea for InMemoryStagingArea {
    fn commit(&mut self) {
        todo!()
    }
}
