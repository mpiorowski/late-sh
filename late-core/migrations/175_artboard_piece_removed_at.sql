-- A piece comes down by soft delete: the row stays for the daily cap, the
-- per-month duplicate rail, and the audit trail; every listing and lookup
-- reads only rows with removed_at IS NULL. Set by the hanger (this month
-- only) or a mod (/mod artboard remove).
ALTER TABLE artboard_pieces ADD COLUMN removed_at timestamptz;
