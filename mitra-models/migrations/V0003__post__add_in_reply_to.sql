ALTER TABLE post ADD COLUMN in_reply_to_id UUID REFERENCES post (id) ON DELETE CASCADE;