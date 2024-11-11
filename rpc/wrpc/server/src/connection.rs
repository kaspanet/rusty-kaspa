use kaspa_notify::{
    connection::Connection as ConnectionT,
    error::{Error as NotifyError, Result as NotifyResult},
    listener::ListenerId,
    notification::Notification as NotificationT,
    notifier::Notify,
};
use kaspa_rpc_core::{api::ops::RpcApiOps, Notification};
use std::{
    fmt::{Debug, Display},
    sync::{Arc, Mutex},
};
use workflow_log::log_trace;
use workflow_rpc::{
    server::{prelude::*, result::Result as WrpcResult},
    types::{MsgT, OpsT},
};
use workflow_serializer::prelude::*;

//
// FIXME: Use workflow_rpc::encoding::Encoding directly in the ConnectionT implementation by deriving Hash, Eq and PartialEq in situ
//
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum NotifyEncoding {
    Borsh,
    SerdeJson,
}
impl From<Encoding> for NotifyEncoding {
    fn from(value: Encoding) -> Self {
        match value {
            Encoding::Borsh => NotifyEncoding::Borsh,
            Encoding::SerdeJson => NotifyEncoding::SerdeJson,
        }
    }
}
impl From<NotifyEncoding> for Encoding {
    fn from(value: NotifyEncoding) -> Self {
        match value {
            NotifyEncoding::Borsh => Encoding::Borsh,
            NotifyEncoding::SerdeJson => Encoding::SerdeJson,
        }
    }
}

#[derive(Debug)]
struct ConnectionInner {
    pub id: u64,
    pub peer: SocketAddr,
    pub messenger: Arc<Messenger>,
    // not using an atomic in case an Id will change type in the future...
    pub listener_id: Mutex<Option<ListenerId>>,
}

impl ConnectionInner {
    fn send(&self, message: Message) -> crate::result::Result<()> {
        Ok(self.messenger.send_raw_message(message)?)
    }
}

impl Notify<Notification> for ConnectionInner {
    fn notify(&self, notification: Notification) -> NotifyResult<()> {
        self.send(Connection::into_message(&notification, &self.messenger.encoding().into()))
            .map_err(|err| NotifyError::General(err.to_string()))
    }
}

impl Display for ConnectionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.id, self.peer)
    }
}

/// [`Connection`] represents a currently connected WebSocket RPC channel.
/// This struct owns a [`Messenger`] that has [`Messenger::notify`]
/// function that can be used to post notifications to the connection.
/// [`Messenger::close`] function can be used to terminate the connection
/// asynchronously.
#[derive(Debug, Clone)]
pub struct Connection {
    inner: Arc<ConnectionInner>,
}

impl Connection {
    pub fn new(id: u64, peer: &SocketAddr, messenger: Arc<Messenger>) -> Connection {
        Connection { inner: Arc::new(ConnectionInner { id, peer: *peer, messenger, listener_id: None.into() }) }
    }

    /// Obtain the connection id
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    /// Get a reference to the connection [`Messenger`]
    pub fn messenger(&self) -> &Arc<Messenger> {
        &self.inner.messenger
    }

    pub fn listener_id(&self) -> Option<ListenerId> {
        *self.inner.listener_id.lock().unwrap()
    }

    pub fn register_notification_listener(&self, listener_id: ListenerId) {
        self.inner.listener_id.lock().unwrap().replace(listener_id);
    }

    pub fn peer(&self) -> &SocketAddr {
        &self.inner.peer
    }

    /// Creates a WebSocket [`Message`] that can be posted to the connection ([`Messenger`]) sink
    /// directly.
    pub fn create_serialized_notification_message<Ops, Msg>(encoding: Encoding, op: Ops, msg: Msg) -> WrpcResult<Message>
    where
        Ops: OpsT,
        Msg: MsgT,
    {
        match encoding {
            Encoding::Borsh => workflow_rpc::server::protocol::borsh::create_serialized_notification_message(op, msg),
            Encoding::SerdeJson => workflow_rpc::server::protocol::serde_json::create_serialized_notification_message(op, msg),
        }
    }
}

impl Display for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

#[async_trait::async_trait]
impl ConnectionT for Connection {
    type Notification = Notification;
    type Message = Message;
    type Encoding = NotifyEncoding;
    type Error = kaspa_notify::error::Error;

    fn encoding(&self) -> Self::Encoding {
        self.messenger().encoding().into()
    }

    fn into_message(notification: &Self::Notification, encoding: &Self::Encoding) -> Self::Message {
        let op: RpcApiOps = notification.event_type().into();
        Self::create_serialized_notification_message(encoding.clone().into(), op, Serializable(notification.clone())).unwrap()
    }

    async fn send(&self, message: Self::Message) -> core::result::Result<(), Self::Error> {
        self.inner.send(message).map_err(|err| NotifyError::General(err.to_string()))
    }

    fn close(&self) -> bool {
        if !self.is_closed() {
            if let Err(err) = self.messenger().close() {
                log_trace!("Error closing connection {}: {}", self.peer(), err);
            } else {
                return true;
            }
        }
        false
    }

    fn is_closed(&self) -> bool {
        self.messenger().sink().is_closed()
    }
}

pub type ConnectionReference = Arc<Connection>;
