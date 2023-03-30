ALTER TABLE actor_profile ADD COLUMN subscriber_count INTEGER NOT NULL CHECK (subscriber_count >= 0) DEFAULT 0;
UPDATE actor_profile SET subscriber_count = (
    SELECT count(*) FROM relationship WHERE relationship.target_id = actor_profile.id
    AND relationship.relationship_type = 3
);
