ALTER TABLE user_account ADD COLUMN client_config JSONB NOT NULL DEFAULT '{}';
