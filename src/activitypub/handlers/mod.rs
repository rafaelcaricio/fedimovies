use super::fetcher::helpers::ImportError;
// Handlers should return object type if activity has been accepted
// or None if it has been ignored
pub type HandlerResult = Result<Option<&'static str>, ImportError>;

pub mod announce;
pub mod create_note;
pub mod delete;
pub mod follow;
pub mod like;
pub mod update_note;
pub mod update_person;
