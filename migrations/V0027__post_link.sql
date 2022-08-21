CREATE TABLE post_link (
    source_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, target_id)
);
