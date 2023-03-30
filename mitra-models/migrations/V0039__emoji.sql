CREATE TABLE emoji (
    id UUID PRIMARY KEY,
    emoji_name VARCHAR(100) NOT NULL,
    hostname VARCHAR(100) REFERENCES instance (hostname) ON DELETE RESTRICT,
    image JSONB NOT NULL,
    object_id VARCHAR(250) UNIQUE,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    UNIQUE (emoji_name, hostname),
    CHECK ((hostname IS NULL) = (object_id IS NULL))
);

CREATE TABLE post_emoji (
    post_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    emoji_id UUID NOT NULL REFERENCES emoji (id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, emoji_id)
);
