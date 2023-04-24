use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{profiles::types::DbActor, users::types::User};
use mitra_utils::id::generate_ulid;

use crate::activitypub::{
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id, LocalActorCollection},
    types::{build_default_context, Context},
    vocabulary::{ADD, REMOVE},
};

#[derive(Serialize)]
struct AddOrRemovePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,
    id: String,
    object: String,
    target: String,

    to: Vec<String>,
}

fn build_update_collection(
    instance_url: &str,
    sender_username: &str,
    person_id: &str,
    collection: LocalActorCollection,
    remove: bool,
) -> AddOrRemovePerson {
    let actor_id = local_actor_id(instance_url, sender_username);
    let activity_id = local_object_id(instance_url, &generate_ulid());
    let activity_type = if remove { REMOVE } else { ADD };
    let collection_id = collection.of(&actor_id);
    AddOrRemovePerson {
        context: build_default_context(),
        id: activity_id,
        activity_type: activity_type.to_string(),
        actor: actor_id,
        object: person_id.to_string(),
        target: collection_id,
        to: vec![person_id.to_string()],
    }
}

pub fn prepare_update_collection(
    instance: &Instance,
    sender: &User,
    person: &DbActor,
    collection: LocalActorCollection,
    remove: bool,
) -> OutgoingActivity {
    let activity = build_update_collection(
        &instance.url(),
        &sender.profile.username,
        &person.id,
        collection,
        remove,
    );
    let recipients = vec![person.clone()];
    OutgoingActivity::new(instance, sender, activity, recipients)
}

pub fn prepare_add_person(
    instance: &Instance,
    sender: &User,
    person: &DbActor,
    collection: LocalActorCollection,
) -> OutgoingActivity {
    prepare_update_collection(instance, sender, person, collection, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_add_person() {
        let sender_username = "local";
        let person_id = "https://test.remote/actor/test";
        let collection = LocalActorCollection::Subscribers;
        let activity =
            build_update_collection(INSTANCE_URL, sender_username, person_id, collection, false);

        assert_eq!(activity.activity_type, "Add");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, sender_username),
        );
        assert_eq!(activity.object, person_id);
        assert_eq!(
            activity.target,
            format!("{}/users/{}/subscribers", INSTANCE_URL, sender_username),
        );
        assert_eq!(activity.to[0], person_id);
    }
}
