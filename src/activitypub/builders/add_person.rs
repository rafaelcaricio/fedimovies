use serde::Serialize;

use crate::activitypub::{
    actors::types::Actor,
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id, LocalActorCollection},
    vocabulary::{ADD, REMOVE},
};
use crate::config::Instance;
use crate::models::users::types::User;
use crate::utils::id::new_uuid;

#[derive(Serialize)]
pub struct AddOrRemovePerson {
    #[serde(rename = "@context")]
    context: String,

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
    let activity_id = local_object_id(instance_url, &new_uuid());
    let activity_type = if remove { REMOVE } else { ADD };
    let collection_id = collection.of(&actor_id);
    AddOrRemovePerson {
        context: AP_CONTEXT.to_string(),
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
    person: &Actor,
    collection: LocalActorCollection,
    remove: bool,
) -> OutgoingActivity<AddOrRemovePerson> {
    let activity = build_update_collection(
        &instance.url(),
        &sender.profile.username,
        &person.id,
        collection,
        remove,
    );
    let recipients = vec![person.clone()];
    OutgoingActivity {
        instance: instance.clone(),
        sender: sender.clone(),
        activity,
        recipients,
    }
}

pub fn prepare_add_person(
    instance: &Instance,
    sender: &User,
    person: &Actor,
    collection: LocalActorCollection,
) -> OutgoingActivity<AddOrRemovePerson> {
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
        let activity = build_update_collection(
            INSTANCE_URL,
            sender_username,
            person_id,
            collection,
            false,
        );

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