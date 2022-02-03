ALTER TABLE relationship ADD COLUMN relationship_type SMALLINT NOT NULL DEFAULT 1;
ALTER TABLE relationship ALTER COLUMN relationship_type DROP DEFAULT;
ALTER TABLE relationship DROP CONSTRAINT relationship_pkey;
ALTER TABLE relationship ADD PRIMARY KEY (id);
ALTER TABLE relationship ADD CONSTRAINT relationship_source_id_target_id_relationship_type_key UNIQUE (source_id, target_id, relationship_type);
