ALTER TABLE actor_profile ADD COLUMN actor_id VARCHAR(200) UNIQUE GENERATED ALWAYS AS (actor_json ->> 'id') STORED;
