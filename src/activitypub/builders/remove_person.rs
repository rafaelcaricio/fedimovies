use mitra_config::Instance;

use crate::activitypub::{
    deliverer::OutgoingActivity,
    identifiers::LocalActorCollection,
};
use crate::models::{
    profiles::types::DbActor,
    users::types::User,
};
use super::add_person::prepare_update_collection;

pub fn prepare_remove_person(
    instance: &Instance,
    sender: &User,
    person: &DbActor,
    collection: LocalActorCollection,
) -> OutgoingActivity {
    prepare_update_collection(instance, sender, person, collection, true)
}
