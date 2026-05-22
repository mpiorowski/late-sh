-- Add updated column to pinstar_invites to match model! macro expectations
ALTER TABLE pinstar_invites ADD COLUMN updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP;
