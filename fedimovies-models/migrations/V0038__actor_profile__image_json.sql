ALTER TABLE actor_profile ADD COLUMN avatar JSONB;
ALTER TABLE actor_profile ADD COLUMN banner JSONB;
UPDATE actor_profile
    SET avatar = json_build_object('file_name', avatar_file_name)
    WHERE avatar_file_name IS NOT NULL;
UPDATE actor_profile
    SET banner = json_build_object('file_name', banner_file_name)
    WHERE banner_file_name IS NOT NULL;
ALTER TABLE actor_profile DROP COLUMN avatar_file_name;
ALTER TABLE actor_profile DROP COLUMN banner_file_name;
