CREATE TABLE invoice (
    id UUID PRIMARY KEY,
    sender_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    chain_id VARCHAR(50) NOT NULL,
    payment_address VARCHAR(200) NOT NULL,
    invoice_status SMALLINT NOT NULL DEFAULT 1,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (chain_id, payment_address)
);
