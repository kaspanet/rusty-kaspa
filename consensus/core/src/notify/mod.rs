use crate::block::Block;

#[derive(Debug, Clone)]
pub enum Notification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}
