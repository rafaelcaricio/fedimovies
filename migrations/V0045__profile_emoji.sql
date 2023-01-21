CREATE TABLE profile_emoji (
    profile_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    emoji_id UUID NOT NULL REFERENCES emoji (id) ON DELETE CASCADE,
    PRIMARY KEY (profile_id, emoji_id)
);
