#[allow(dead_code)]
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
