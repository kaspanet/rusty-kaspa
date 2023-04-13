use async_trait::async_trait;

use crate::notification::Notification;
use core::fmt::Debug;
use std::marker::PhantomData;

#[async_trait]
pub trait Converter: Send + Sync + Debug {
    type Incoming: Send + Sync + 'static + Sized + Debug;
    type Outgoing: Notification;

    async fn convert(&self, incoming: Self::Incoming) -> Self::Outgoing;
}

/// A notification [`Converter`] that converts an incoming `I` into a notification `N` using the [`From`] trait.
#[derive(Debug)]
pub struct ConverterFrom<I, N>
where
    I: Send + Sync + 'static + Sized + Debug,
    N: Notification,
{
    _incoming: PhantomData<I>,
    _notification: PhantomData<N>,
}

impl<I, N> ConverterFrom<I, N>
where
    N: Notification,
    I: Send + Sync + 'static + Sized + Debug,
{
    pub fn new() -> Self {
        Self { _incoming: PhantomData, _notification: PhantomData }
    }
}

impl<I, N> Default for ConverterFrom<I, N>
where
    N: Notification,
    I: Send + Sync + 'static + Sized + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<I, N> Converter for ConverterFrom<I, N>
where
    N: Notification,
    I: Send + Sync + 'static + Sized + Debug,
    I: Into<N>,
{
    type Incoming = I;
    type Outgoing = N;

    async fn convert(&self, incoming: I) -> N {
        incoming.into()
    }
}
