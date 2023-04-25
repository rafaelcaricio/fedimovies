CREATE TABLE post_reaction (
    id UUID PRIMARY KEY,
    author_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    post_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    UNIQUE (author_id, post_id)
);
ALTER TABLE post ADD COLUMN reaction_count INTEGER NOT NULL CHECK (reaction_count >= 0) DEFAULT 0;
