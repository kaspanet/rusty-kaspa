use async_channel::TrySendError;
use async_channel::{unbounded, Receiver, Sender};

/// Multiple producers multiple consumers channel
#[derive(Clone, Debug)]
pub struct Channel<T> {
    pub sender: Sender<T>,
    pub receiver: Receiver<T>,
}

impl<T> Channel<T> {
    pub fn new(channel: (Sender<T>, Receiver<T>)) -> Channel<T> {
        Self { sender: channel.0, receiver: channel.1 }
    }

    pub fn sender(&self) -> Sender<T> {
        self.sender.clone()
    }

    pub fn receiver(&self) -> Receiver<T> {
        self.receiver.clone()
    }

    pub fn close(&self) {
        self.receiver.close();
    }

    pub fn is_closed(&self) -> bool {
        self.receiver.is_closed()
    }

    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(msg)
    }
}

/// Default for a [`Channel<T>`] is unbounded
impl<T> Default for Channel<T> {
    fn default() -> Self {
        let ch = unbounded();
        Self { sender: ch.0, receiver: ch.1 }
    }
}
