use crate::activitypub::constants::AP_PUBLIC;
use crate::models::posts::types::Visibility;
use crate::models::profiles::types::DbActorProfile;

pub fn get_note_visibility(
    note_id: &str,
    author: &DbActorProfile,
    primary_audience: Vec<String>,
    secondary_audience: Vec<String>,
) -> Visibility {
    if primary_audience.contains(&AP_PUBLIC.to_string()) ||
            secondary_audience.contains(&AP_PUBLIC.to_string()) {
        Visibility::Public
    } else {
        // Treat all notes that aren't public-addressed as direct messages
        log::warn!(
            "processing non-public note {} attributed to {}; primary audience {:?}; secondary audience {:?}",
            note_id,
            author.username,
            primary_audience,
            secondary_audience,
        );
        Visibility::Direct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_note_visibility_public() {
        let object_id = "https://example.com/test";
        let author = DbActorProfile::default();
        let primary_audience = vec![AP_PUBLIC.to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            object_id,
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Public);
    }

    #[test]
    fn test_get_note_visibility_direct() {
        let object_id = "https://example.com/test";
        let author = DbActorProfile::default();
        let primary_audience = vec!["https://example.com/users/1".to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            object_id,
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Direct);
    }
}
