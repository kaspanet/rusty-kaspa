use async_channel::{bounded, unbounded, Receiver, RecvError, SendError, Sender, TryRecvError, TrySendError};

/// Multiple producers multiple consumers channel
#[derive(Clone, Debug)]
pub struct Channel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> Channel<T> {
    pub fn new(channel: (Sender<T>, Receiver<T>)) -> Channel<T> {
        Self { sender: channel.0, receiver: channel.1 }
    }

    pub fn bounded(capacity: usize) -> Channel<T> {
        let channel = bounded(capacity);
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

    pub async fn recv(&self) -> Result<T, RecvError> {
        self.receiver.recv().await
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }

    pub async fn send(&self, msg: T) -> Result<(), SendError<T>> {
        self.sender.send(msg).await
    }

    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(msg)
    }

    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    pub fn sender_count(&self) -> usize {
        self.sender.sender_count()
    }
}

/// Default for a [`Channel<T>`] is unbounded
impl<T> Default for Channel<T> {
    fn default() -> Self {
        let ch = unbounded();
        Self { sender: ch.0, receiver: ch.1 }
    }
}
