use crate::activitypub::constants::AP_PUBLIC;
use crate::models::posts::types::Visibility;
use crate::models::profiles::types::DbActorProfile;

pub fn get_note_visibility(
    author: &DbActorProfile,
    primary_audience: Vec<String>,
    secondary_audience: Vec<String>,
) -> Visibility {
    if primary_audience.contains(&AP_PUBLIC.to_string()) ||
            secondary_audience.contains(&AP_PUBLIC.to_string()) {
        Visibility::Public
    } else {
        let maybe_followers = author.actor_json.as_ref()
            .and_then(|actor| actor.followers.as_ref());
        if let Some(followers) = maybe_followers {
            if primary_audience.contains(&followers) ||
                    secondary_audience.contains(&followers) {
                Visibility::Followers
            } else {
                Visibility::Direct
            }
        } else {
            Visibility::Direct
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::activitypub::actor::Actor;
    use super::*;

    #[test]
    fn test_get_note_visibility_public() {
        let author = DbActorProfile::default();
        let primary_audience = vec![AP_PUBLIC.to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Public);
    }

    #[test]
    fn test_get_note_visibility_followers() {
        let author_followers = "https://example.com/users/author/followers";
        let author = DbActorProfile {
            actor_json: Some(Actor {
                followers: Some(author_followers.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let primary_audience = vec![author_followers.to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Followers);
    }

    #[test]
    fn test_get_note_visibility_direct() {
        let author = DbActorProfile::default();
        let primary_audience = vec!["https://example.com/users/1".to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Direct);
    }
}
