/// URL builders for default frontend
use uuid::Uuid;

// Assuming frontend is on the same host as backend
pub fn get_profile_page_url(profile_id: &Uuid, instance_url: &str) -> String {
    format!("{}/profile/{}", instance_url, profile_id)
}

pub fn get_post_page_url(post_id: &Uuid, instance_url: &str) -> String {
    format!("{}/post/{}", instance_url, post_id)
}
