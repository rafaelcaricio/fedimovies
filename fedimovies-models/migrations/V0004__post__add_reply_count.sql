ALTER TABLE post ADD COLUMN reply_count INTEGER NOT NULL CHECK (reply_count >= 0) DEFAULT 0;
