CREATE TABLE notification (
    id SERIAL PRIMARY KEY,
    sender_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    post_id UUID REFERENCES post (id) ON DELETE CASCADE,
    event_type SMALLINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);
