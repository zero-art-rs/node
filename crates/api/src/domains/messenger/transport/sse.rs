use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream::Stream;
use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;
use tracing::error;
use utoipa::{IntoParams, ToSchema};

use crate::Container;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeForMessagesQuery {
    pub chat_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/subscribe",
    params(
        SubscribeForMessagesQuery
    ),
    responses(
        (status = 200, description = "Successfully subscribed to message stream", content_type = "text/event-stream"),
        (status = 500, description = "Failed to subscribe to messages", body = String)
    ),
    tag = "Messenger",
    summary = "Subscribe to real-time messages",
    description = "Subscribe to a Server-Sent Events (SSE) stream to receive real-time messages for a specific chat room. The stream will send JSON-formatted message events as they are added to the chat."
)]
pub async fn subscribe_for_messages(
    State(state): State<Arc<Container>>,
    Query(payload): Query<SubscribeForMessagesQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Response> {
    let message_stream = match state
        .messenger_service
        .subscribe_for_messages(&payload.chat_id)
        .await
    {
        Ok(rx) => {
            let stream = ReceiverStream::new(rx).map(|message| {
                Ok(Event::default().data(
                    serde_json::to_string(&message).unwrap_or_else(|_| {
                        format!("{{\"error\": \"Failed to serialize message\"}}")
                    }),
                ))
            });
            stream
        }
        Err(e) => {
            error!("Failed to subscribe to messages: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to subscribe to messages",
            )
                .into_response());
        }
    };

    Ok(Sse::new(message_stream).keep_alive(KeepAlive::default()))
}
