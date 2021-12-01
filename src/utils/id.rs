use ulid::Ulid;
use uuid::Uuid;

/// Produces new lexicographically sortable ID
pub fn new_uuid() -> Uuid {
    let ulid = Ulid::new();
    Uuid::from(ulid)
}
