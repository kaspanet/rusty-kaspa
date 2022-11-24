use crate::block::Block;

#[derive(Debug, Clone)]
pub enum Notification {
    BlockAdded(BlockAddedNotification),
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}
