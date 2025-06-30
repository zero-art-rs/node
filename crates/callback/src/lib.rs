//! Zk Messenger callbacks

use eyre::eyre;
use tokio::sync::{mpsc, oneshot};

/// Type definition for the oneshot response channel carrying callback results
///
/// This type alias is used to represent the sender end of a oneshot channel,
/// which is used to send and receive results from asynchronous operations.
pub type CallbackSender<V> = oneshot::Sender<V>;

/// Wrapper struct for carrying a callback and its sender
///
/// This struct is used to carry a callback and its sender together,
/// allowing for the callback to be sent and received from asynchronous operations.
pub struct CallbackWrapper<T, V>(T, CallbackSender<V>);

impl<T, V> CallbackWrapper<T, V> {
    /// Returns a tuple containing the callback and its sender
    ///
    /// This method returns a tuple containing the callback and its sender,
    /// allowing for the callback to be accessed and used in the calling code.
    pub fn inner(&self) -> (&T, &CallbackSender<V>) {
        (&self.0, &self.1)
    }

    /// Returns a tuple containing the callback and its sender
    ///
    /// This method returns a tuple containing the callback and its sender,
    /// allowing for the callback to be accessed and used in the calling code.
    pub fn inner_owned(self) -> (T, CallbackSender<V>) {
        (self.0, self.1)
    }
}

impl<T, V> From<(T, CallbackSender<V>)> for CallbackWrapper<T, V> {
    fn from(value: (T, CallbackSender<V>)) -> Self {
        CallbackWrapper(value.0, value.1)
    }
}

/// Sends a message to a processor and returns the result
///
/// This function sends a message to a processor and returns the result of the operation.
/// It uses a oneshot channel to send the message and receive the result.
pub async fn callback<M, R>(
    processor_tx: &mpsc::Sender<CallbackWrapper<M, eyre::Result<R>>>,
    message: M,
) -> eyre::Result<R> {
    let (callback_tx, callback_rx) = oneshot::channel();

    processor_tx
        .send((message, callback_tx).into())
        .await
        .map_err(|err| eyre!("Failed to send message to processor: {}", err))?;

    callback_rx
        .await
        .map_err(|err| eyre!("Failed to receive response from processor: {}", err))?
}
