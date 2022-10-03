CREATE TABLE instance (
    hostname VARCHAR(100) PRIMARY KEY
);
INSERT INTO instance
    SELECT DISTINCT split_part(acct, '@', 2)
    FROM actor_profile
    WHERE acct <> username;
ALTER TABLE actor_profile
    ADD COLUMN hostname VARCHAR(100) REFERENCES instance (hostname) ON DELETE RESTRICT;
UPDATE actor_profile
    SET hostname = split_part(acct, '@', 2)
    WHERE acct <> username;
ALTER TABLE actor_profile
    ADD CONSTRAINT actor_profile_hostname_check CHECK ((hostname IS NULL) = (actor_json IS NULL));
ALTER TABLE actor_profile
    DROP COLUMN acct;
ALTER TABLE actor_profile
    ADD COLUMN acct VARCHAR(200) UNIQUE
    GENERATED ALWAYS AS (CASE WHEN hostname IS NULL THEN username ELSE username || '@' || hostname END) STORED;
