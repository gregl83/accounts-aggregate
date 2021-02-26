use simple_error::SimpleError;

/// Handles `Causes` by producing `Effects`.
///
/// Handle receives `causes` and returns `effects`.
/// Apply receives `effects`.
pub trait Actor<C: Cause, E: Effect> {
    type Id;
    fn handle(&self, command: C) -> Result<Vec<E>, SimpleError>;
    fn apply(&mut self, events: Vec<E>);
}

/// Contributes to production of an Effect.
pub trait Cause {
    type ActorId;
    fn actor_id(&self) -> Self::ActorId;
}

/// Produced by Cause and effects state of an entity.
pub trait Effect {
    type Version;
    type Key;
    fn version(&self) -> Self::Version;
    fn idempotency_key(&self) -> Self::Key;
}