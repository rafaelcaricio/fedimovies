ALTER TABLE post ADD COLUMN repost_of_id UUID REFERENCES post (id) ON DELETE CASCADE;
ALTER TABLE post ADD COLUMN repost_count INTEGER NOT NULL CHECK (repost_count >= 0) DEFAULT 0;
ALTER TABLE post ADD CONSTRAINT post_author_id_repost_of_id_key UNIQUE (author_id, repost_of_id);
