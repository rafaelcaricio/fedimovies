CREATE TABLE background_job (
    id UUID PRIMARY KEY,
    job_type SMALLINT NOT NULL,
    job_data JSONB NOT NULL,
    job_status SMALLINT NOT NULL DEFAULT 1,
    scheduled_for TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE instance (
    hostname VARCHAR(100) PRIMARY KEY
);

CREATE TABLE actor_profile (
    id UUID PRIMARY KEY,
    username VARCHAR(100) NOT NULL,
    hostname VARCHAR(100) REFERENCES instance (hostname) ON DELETE RESTRICT,
    acct VARCHAR(200) UNIQUE GENERATED ALWAYS AS (CASE WHEN hostname IS NULL THEN username ELSE username || '@' || hostname END) STORED,
    display_name VARCHAR(200),
    bio TEXT,
    bio_source TEXT,
    avatar_file_name VARCHAR(100),
    banner_file_name VARCHAR(100),
    identity_proofs JSONB NOT NULL DEFAULT '[]',
    payment_options JSONB NOT NULL DEFAULT '[]',
    extra_fields JSONB NOT NULL DEFAULT '[]',
    follower_count INTEGER NOT NULL CHECK (follower_count >= 0) DEFAULT 0,
    following_count INTEGER NOT NULL CHECK (following_count >= 0) DEFAULT 0,
    subscriber_count INTEGER NOT NULL CHECK (subscriber_count >= 0) DEFAULT 0,
    post_count INTEGER NOT NULL CHECK (post_count >= 0) DEFAULT 0,
    actor_json JSONB,
    actor_id VARCHAR(200) UNIQUE GENERATED ALWAYS AS (actor_json ->> 'id') STORED,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((hostname IS NULL) = (actor_json IS NULL))
);

CREATE TABLE user_invite_code (
    code VARCHAR(100) PRIMARY KEY,
    used BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE user_account (
    id UUID PRIMARY KEY REFERENCES actor_profile (id) ON DELETE CASCADE,
    wallet_address VARCHAR(100) UNIQUE,
    password_hash VARCHAR(200),
    private_key TEXT NOT NULL,
    invite_code VARCHAR(100) UNIQUE REFERENCES user_invite_code (code) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE oauth_token (
    id SERIAL PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    token VARCHAR(100) UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL
);

CREATE TABLE relationship (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    source_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    relationship_type SMALLINT NOT NULL,
    UNIQUE (source_id, target_id, relationship_type)
);

CREATE TABLE follow_request (
    id UUID PRIMARY KEY,
    source_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    activity_id VARCHAR(250) UNIQUE,
    request_status SMALLINT NOT NULL,
    UNIQUE (source_id, target_id)
);

CREATE TABLE post (
    id UUID PRIMARY KEY,
    author_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    in_reply_to_id UUID REFERENCES post (id) ON DELETE CASCADE,
    repost_of_id UUID REFERENCES post (id) ON DELETE CASCADE,
    visilibity SMALLINT NOT NULL,
    reply_count INTEGER NOT NULL CHECK (reply_count >= 0) DEFAULT 0,
    reaction_count INTEGER NOT NULL CHECK (reaction_count >= 0) DEFAULT 0,
    repost_count INTEGER NOT NULL CHECK (repost_count >= 0) DEFAULT 0,
    object_id VARCHAR(200) UNIQUE,
    ipfs_cid VARCHAR(200),
    token_id INTEGER,
    token_tx_id VARCHAR(200),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE,
    UNIQUE (author_id, repost_of_id)
);

CREATE TABLE post_reaction (
    id UUID PRIMARY KEY,
    author_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    post_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    activity_id VARCHAR(250) UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    UNIQUE (author_id, post_id)
);

CREATE TABLE media_attachment (
    id UUID PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    media_type VARCHAR(50),
    file_name VARCHAR(200) NOT NULL,
    ipfs_cid VARCHAR(200),
    post_id UUID REFERENCES post (id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE mention (
    post_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, profile_id)
);

CREATE TABLE tag (
    id SERIAL PRIMARY KEY,
    tag_name VARCHAR(100) UNIQUE NOT NULL
);

CREATE TABLE post_tag (
    post_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tag (id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);

CREATE TABLE post_link (
    source_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES post (id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, target_id)
);

CREATE TABLE notification (
    id SERIAL PRIMARY KEY,
    sender_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    post_id UUID REFERENCES post (id) ON DELETE CASCADE,
    event_type SMALLINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE timeline_marker (
    id SERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    timeline SMALLINT NOT NULL,
    last_read_id VARCHAR(100) NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    UNIQUE (user_id, timeline)
);

CREATE TABLE invoice (
    id UUID PRIMARY KEY,
    sender_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    chain_id VARCHAR(50) NOT NULL,
    payment_address VARCHAR(200) NOT NULL,
    amount BIGINT NOT NULL CHECK (amount >= 0),
    invoice_status SMALLINT NOT NULL DEFAULT 1,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (chain_id, payment_address)
);

CREATE TABLE subscription (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sender_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    sender_address VARCHAR(100),
    recipient_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    chain_id VARCHAR(50) NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    UNIQUE (sender_id, recipient_id)
);
