use std::collections::HashMap;

use futures_util::StreamExt;
use mongodb::change_stream::{
    ChangeStream,
    event::{ChangeStreamEvent, OperationType},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use types::{Message, Subscription};

#[derive(Debug)]
pub struct MessageWatcher {
    /// Receiver of chat ids to watch
    subscription_receiver: mpsc::Receiver<Subscription>,
    /// Real-time senders of messages
    /// chat_id -> [sender1, sender2, ...]
    message_sender: HashMap<String, Vec<mpsc::Sender<Message>>>,

    inner_tx: mpsc::Sender<(String, Message)>,
    inner_rx: mpsc::Receiver<(String, Message)>,

    tasks: HashMap<String, JoinHandle<()>>,
}

impl MessageWatcher {
    pub fn new(subscription_receiver: mpsc::Receiver<Subscription>) -> Self {
        let (inner_tx, inner_rx) = mpsc::channel(100);

        Self {
            subscription_receiver,
            message_sender: HashMap::new(),
            inner_tx,
            inner_rx,
            tasks: HashMap::new(),
        }
    }

    pub async fn run(mut self, cancellation: CancellationToken) {
        loop {
            tokio::select! {
                subscription_opt = self.subscription_receiver.recv() => {
                    let Some(subscription) = subscription_opt else {
                        info!("Subscription receiver closed");
                        break;
                    };

                    info!("Received subscription for chat: {}", subscription.chat_id);

                    let chat_id = subscription.chat_id.clone();
                    let sender = subscription.sender.clone();
                    let change_stream = subscription.change_stream;

                    let receivers = self
                        .message_sender
                        .entry(chat_id.clone())
                        .or_insert(Vec::new());
                    receivers.push(sender);

                    debug!(
                        chat_id = chat_id.as_str(),
                        receivers = ?receivers,
                        "Updated message senders for chat"
                    );

                    if !self.tasks.contains_key(&chat_id) {
                        self.spawn_subscription_task(chat_id, change_stream).await;
                    }
                }
                message = self.inner_rx.recv() => {
                    let Some((chat_id, message)) = message else {
                        error!("Inner receiver closed");
                        break;
                    };

                    if let Err(e) = self.broadcast_message(chat_id, message).await {
                        error!("Error broadcasting message: {}", e);
                    }
                }
                _ = cancellation.cancelled() => {
                    info!("Cancellation received");
                    break;
                }
            }
        }
    }

    async fn broadcast_message(
        &mut self,
        chat_id: String,
        message: Message,
    ) -> Result<(), mpsc::error::SendError<Message>> {
        let Some(recievers) = self.message_sender.get(&chat_id) else {
            return Ok(());
        };

        let mut stale_receivers = Vec::new();

        for receiver in recievers {
            if let Err(e) = receiver.send(message.clone()).await {
                stale_receivers.push(true);

                debug!(
                    chat_id = chat_id.as_str(),
                    receiver = ?receiver,
                    "Stale receiver: {}",
                    e
                );

                continue;
            }

            stale_receivers.push(false);
        }

        if !stale_receivers.is_empty() {
            self.remove_stale_receivers(&chat_id, stale_receivers).await;
        }

        info!(
            chat_id = chat_id.as_str(),
            message = ?message,
            "Broadcasted message",
        );

        Ok(())
    }

    async fn remove_stale_receivers(&mut self, chat_id: &str, stale_receivers: Vec<bool>) {
        let Some(recievers) = self.message_sender.get(chat_id) else {
            return;
        };

        let mut new_receivers = Vec::new();

        for (index, receiver) in recievers.iter().enumerate() {
            if !stale_receivers[index] {
                new_receivers.push(receiver.clone());
            }
        }

        self.message_sender
            .insert(chat_id.to_string(), new_receivers.clone());

        if new_receivers.is_empty() && self.tasks.contains_key(chat_id) {
            if let Some(task) = self.tasks.remove(chat_id) {
                task.abort();
            }
        }
    }

    async fn spawn_subscription_task(
        &mut self,
        chat_id: String,
        mut change_stream: ChangeStream<ChangeStreamEvent<Message>>,
    ) {
        info!(
            chat_id = chat_id.as_str(),
            "Spawning subscription task for chat"
        );

        let tx = self.inner_tx.clone();
        let chat_id_clone = chat_id.clone();
        let task = tokio::spawn(async move {
            while let Some(event) = change_stream.next().await {
                match event {
                    Ok(event) => match event.operation_type {
                        OperationType::Insert => {
                            let Some(message) = event.full_document else {
                                continue;
                            };

                            tx.send((chat_id_clone.clone(), message)).await.unwrap();
                        }
                        // Ignore other events
                        _ => {}
                    },
                    Err(err) => {
                        error!("Error in change stream: {}", err);
                    }
                }
            }
        });

        self.tasks.insert(chat_id, task);
    }
}
