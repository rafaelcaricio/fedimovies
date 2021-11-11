CREATE TABLE mention (
    post_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, profile_id)
);
