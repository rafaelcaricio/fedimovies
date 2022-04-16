/// https://www.w3.org/TR/activitystreams-vocabulary/

// Activity types
pub const ACCEPT: &str = "Accept";
pub const ANNOUNCE: &str = "Announce";
pub const CREATE: &str = "Create";
pub const DELETE: &str = "Delete";
pub const EMOJI_REACT: &str = "EmojiReact";
pub const FOLLOW: &str = "Follow";
pub const LIKE: &str = "Like";
pub const REJECT: &str = "Reject";
pub const UNDO: &str = "Undo";
pub const UPDATE: &str = "Update";

// Actor types
pub const PERSON: &str = "Person";
pub const SERVICE: &str = "Service";

// Object types
pub const DOCUMENT: &str = "Document";
pub const IMAGE: &str = "Image";
pub const MENTION: &str = "Mention";
pub const NOTE: &str = "Note";
pub const TOMBSTONE: &str = "Tombstone";

// Collections
pub const ORDERED_COLLECTION: &str = "OrderedCollection";
pub const ORDERED_COLLECTION_PAGE: &str = "OrderedCollectionPage";

// Misc
pub const HASHTAG: &str = "Hashtag";
pub const IDENTITY_PROOF: &str = "IdentityProof";
pub const PROPERTY_VALUE: &str = "PropertyValue";
