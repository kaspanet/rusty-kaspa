use async_channel::{bounded, unbounded, Receiver, RecvError, SendError, Sender, TryRecvError, TrySendError, WeakReceiver};

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

/// Creates a special `job` channel where the sender might replace a previous pending job
/// not consumed yet by the receiver. The internal channel has capacity of `1` but senders
/// can attempt to replace the current `job` via `selector` logic. See [`JobSender::try_send`]
pub fn job<T>() -> (JobSender<T>, JobReceiver<T>) {
    let (send, recv) = bounded(1);
    (JobSender { sender: send, receiver: recv.downgrade() }, recv)
}

pub type JobReceiver<T> = Receiver<T>;

pub type JobTrySendError<T> = TrySendError<T>;

pub type JobTryRecvError = TryRecvError;

/// The sending side of a [`job`] channel.
#[derive(Clone)]
pub struct JobSender<T> {
    sender: Sender<T>,
    receiver: WeakReceiver<T>, // Avoid holding a strong receiver so that the channel will close when all actual receivers drop
}

impl<T> JobSender<T> {
    /// Attempts to send a message into the job channel. If the channel already contains a message, `selector`
    /// is applied to choose which one of them remains. Parallel senders might result in undefined message
    /// selection, the failing sender will receive `TrySendError::Full`.
    ///
    /// If the channel is closed, this method returns an error.
    pub fn try_send<F>(&self, mut msg: T, mut selector: F) -> Result<(), JobTrySendError<T>>
    where
        F: FnMut(T, T) -> T,
    {
        if let Some(receiver) = self.receiver.upgrade() {
            while let Ok(prv) = receiver.try_recv() {
                msg = selector(prv, msg);
            }
        }
        self.sender.try_send(msg)
    }
}
