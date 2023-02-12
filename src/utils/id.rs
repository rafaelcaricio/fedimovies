use ulid::Ulid;
use uuid::Uuid;

/// Produces new lexicographically sortable ID
pub fn generate_ulid() -> Uuid {
    let ulid = Ulid::new();
    Uuid::from(ulid)
}
