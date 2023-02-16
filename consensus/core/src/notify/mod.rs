use crate::block::Block;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Notification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Arc<Block>,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}
