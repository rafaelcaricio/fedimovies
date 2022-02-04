CREATE TABLE subscription (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sender_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    sender_address VARCHAR(100) NOT NULL,
    recipient_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    UNIQUE (sender_id, recipient_id)
);
