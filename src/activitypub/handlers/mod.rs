pub use super::receiver::HandlerError;
// Handlers should return object type if activity has been accepted
// or None if it has been ignored
pub type HandlerResult = Result<Option<&'static str>, HandlerError>;

pub mod accept_follow;
pub mod add;
pub mod announce;
pub mod create_note;
pub mod delete;
pub mod follow;
pub mod like;
pub mod move_person;
pub mod reject_follow;
pub mod remove;
pub mod undo;
pub mod undo_follow;
pub mod update;
mod update_note;
pub mod update_person;
