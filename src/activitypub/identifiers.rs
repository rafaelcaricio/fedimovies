use uuid::Uuid;

pub enum LocalActorCollection {
    Inbox,
    Outbox,
    Followers,
    Following,
    Subscribers,
}

impl LocalActorCollection {
    pub fn of(&self, actor_id: &str) -> String {
        let name = match self {
            Self::Inbox => "inbox",
            Self::Outbox => "outbox",
            Self::Followers => "followers",
            Self::Following => "following",
            Self::Subscribers => "subscribers",
        };
        format!("{}/{}", actor_id, name)
    }
}

pub fn local_actor_id(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}", instance_url, username)
}

pub fn local_actor_inbox(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Inbox.of(&actor_id)
}

pub fn local_actor_outbox(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Outbox.of(&actor_id)
}

pub fn local_actor_followers(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Followers.of(&actor_id)
}

pub fn local_actor_following(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Following.of(&actor_id)
}

pub fn local_actor_subscribers(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Subscribers.of(&actor_id)
}

pub fn local_instance_actor_id(instance_url: &str) -> String {
    format!("{}/actor", instance_url)
}

pub fn local_object_id(instance_url: &str, internal_object_id: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}
