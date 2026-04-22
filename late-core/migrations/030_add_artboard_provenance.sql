ALTER TABLE artboard_snapshots
ADD COLUMN provenance JSONB NOT NULL DEFAULT '{"cells":[]}'::jsonb;
