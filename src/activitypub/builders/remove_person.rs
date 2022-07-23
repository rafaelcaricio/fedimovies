use crate::activitypub::{
    actors::types::Actor,
    deliverer::OutgoingActivity,
    identifiers::LocalActorCollection,
};
use crate::config::Instance;
use crate::models::users::types::User;
use super::add_person::{prepare_update_collection, AddOrRemovePerson};

pub fn prepare_remove_person(
    instance: &Instance,
    sender: &User,
    person: &Actor,
    collection: LocalActorCollection,
) -> OutgoingActivity<AddOrRemovePerson> {
    prepare_update_collection(instance, sender, person, collection, true)
}
