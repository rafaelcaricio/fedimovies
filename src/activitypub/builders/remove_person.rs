use fedimovies_config::Instance;
use fedimovies_models::{profiles::types::DbActor, users::types::User};

use crate::activitypub::{deliverer::OutgoingActivity, identifiers::LocalActorCollection};

use super::add_person::prepare_update_collection;

pub fn prepare_remove_person(
    instance: &Instance,
    sender: &User,
    person: &DbActor,
    collection: LocalActorCollection,
) -> OutgoingActivity {
    prepare_update_collection(instance, sender, person, collection, true)
}
