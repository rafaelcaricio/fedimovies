ALTER TABLE actor_profile ADD COLUMN manually_approves_followers BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE actor_profile ALTER COLUMN manually_approves_followers DROP DEFAULT;
