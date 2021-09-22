CREATE TABLE actor_profile (
    id UUID PRIMARY KEY,
    username VARCHAR(100) NOT NULL,
    display_name VARCHAR(100),
    acct VARCHAR(200) UNIQUE NOT NULL,
    bio TEXT,
    bio_source TEXT,
    avatar_file_name VARCHAR(100),
    banner_file_name VARCHAR(100),
    extra_fields JSONB NOT NULL DEFAULT '[]',
    follower_count INTEGER NOT NULL CHECK (follower_count >= 0) DEFAULT 0,
    following_count INTEGER NOT NULL CHECK (following_count >= 0) DEFAULT 0,
    post_count INTEGER NOT NULL CHECK (post_count >= 0) DEFAULT 0,
    actor_json JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE user_invite_code (
    code VARCHAR(100) PRIMARY KEY,
    used BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE user_account (
    id UUID PRIMARY KEY REFERENCES actor_profile (id) ON DELETE CASCADE,
    wallet_address VARCHAR(100) UNIQUE NOT NULL,
    password_hash VARCHAR(200) NOT NULL,
    private_key TEXT NOT NULL,
    invite_code VARCHAR(100) UNIQUE REFERENCES user_invite_code (code) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE post (
    id UUID PRIMARY KEY,
    author_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    in_reply_to_id UUID REFERENCES post (id) ON DELETE CASCADE,
    ipfs_cid VARCHAR(200),
    token_id INTEGER,
    token_tx_id VARCHAR(200),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE relationship (
    source_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, target_id)
);

CREATE TABLE follow_request (
    id UUID PRIMARY KEY,
    source_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    request_status SMALLINT NOT NULL,
    UNIQUE (source_id, target_id)
);

CREATE TABLE media_attachment (
    id UUID PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    media_type VARCHAR(50),
    file_name VARCHAR(200) NOT NULL,
    post_id UUID REFERENCES post (id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
)
