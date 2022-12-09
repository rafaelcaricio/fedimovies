pub use super::receiver::HandlerError;
// Handlers should return object type if activity has been accepted
// or None if it has been ignored
pub type HandlerResult = Result<Option<&'static str>, HandlerError>;

pub mod accept;
pub mod add;
pub mod announce;
pub mod create;
pub mod delete;
pub mod follow;
pub mod like;
pub mod r#move;
pub mod reject;
pub mod remove;
pub mod undo;
pub mod update;
