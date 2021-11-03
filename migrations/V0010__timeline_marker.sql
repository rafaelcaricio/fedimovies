CREATE TABLE timeline_marker (
    id SERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    timeline SMALLINT NOT NULL,
    last_read_id VARCHAR(100) NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    UNIQUE (user_id, timeline)
);
