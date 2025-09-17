/// A trait that provides session and transaction management support.
///
/// This trait abstracts over a backend that can create sessions and
/// manage transactional workflows (start, commit).
///
/// # Type Parameters
/// * `E` — The error type returned when session or transaction operations fail.
/// * `S` — The type representing a session handle.
#[async_trait::async_trait]
pub trait SessionSupport<E, S>: Send + Sync + Sized {
    async fn start_session(&self) -> Result<S, E>;

    async fn start_transaction(session: &mut S) -> Result<(), E>;

    async fn commit_transaction(session: &mut S) -> Result<(), E>;

}

/// A specialized trait for backends that provide MongoDB session support.
/// This trait is a lighter abstraction compared to [`SessionSupport`],
/// focusing only on session creation without explicit transaction control.
///
/// # Type Parameters
/// * `E` — The error type returned when session operations fail.
/// * `S` — The type representing a MongoDB client session.
#[async_trait::async_trait]
pub trait MongoSessionSupport<E, S> {
    async fn start_session(&self) -> Result<S, E>;
}


