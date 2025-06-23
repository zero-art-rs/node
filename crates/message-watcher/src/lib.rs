use std::collections::HashMap;

use futures_util::StreamExt;
use mongodb::change_stream::{
    ChangeStream,
    event::{ChangeStreamEvent, OperationType},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, error, info};
use types::Message;

#[derive(Debug)]
pub struct MessageWatcher {
    /// Receiver of chat ids to watch
    subscription_receiver: mpsc::Receiver<(
        String,
        ChangeStream<ChangeStreamEvent<Message>>,
        mpsc::Sender<Message>,
    )>,
    /// Real-time senders of messages
    /// chat_id -> [sender1, sender2, ...]
    message_sender: HashMap<String, Vec<mpsc::Sender<Message>>>,

    inner_tx: mpsc::Sender<(String, Message)>,
    inner_rx: mpsc::Receiver<(String, Message)>,

    tasks: Vec<JoinHandle<()>>,
}

impl MessageWatcher {
    pub fn new(
        subscription_receiver: mpsc::Receiver<(
            String,
            ChangeStream<ChangeStreamEvent<Message>>,
            mpsc::Sender<Message>,
        )>,
    ) -> Self {
        let (inner_tx, inner_rx) = mpsc::channel(100);

        Self {
            subscription_receiver,
            message_sender: HashMap::new(),
            inner_tx,
            inner_rx,
            tasks: Vec::new(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                subscription = self.subscription_receiver.recv() => {
                    let Some((chat_id, change_stream, sender)) = subscription else {
                        info!("Subscription receiver closed");
                        break;
                    };

                    info!("Received subscription for chat: {}", chat_id);

                    let receivers = self.message_sender.entry(chat_id.clone()).or_insert(Vec::new());
                    receivers.push(sender);

                    debug!(
                        chat_id = chat_id.as_str(),
                        receivers = ?receivers,
                        "Updated message senders for chat"
                    );

                    if receivers.len() == 1 {
                        self.spawn_subscription_task(chat_id, change_stream).await;
                    }
                }
                message = self.inner_rx.recv() => {
                    let Some((chat_id, message)) = message else {
                        error!("Inner receiver closed");
                        break;
                    };

                    self.broadcast_message(chat_id, message).await;
                }
            }
        }
    }

    async fn broadcast_message(&self, chat_id: String, message: Message) {
        let Some(recievers) = self.message_sender.get(&chat_id) else {
            return;
        };

        for receiver in recievers.iter() {
            receiver.send(message.clone()).await.unwrap();
        }

        info!(
            chat_id = chat_id.as_str(),
            message = ?message,
            "Broadcasted message to {} receivers",
            recievers.len()
        );
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
        let task = tokio::spawn(async move {
            while let Some(event) = change_stream.next().await {
                match event {
                    Ok(event) => match event.operation_type {
                        OperationType::Insert => {
                            let Some(message) = event.full_document else {
                                continue;
                            };

                            tx.send((chat_id.clone(), message)).await.unwrap();
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

        self.tasks.push(task);
    }
}
