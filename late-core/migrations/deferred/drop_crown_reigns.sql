-- DEFERRED: promote to migrations/ (with a fresh number) in the release
-- AFTER the crown-removal code ships. The previous binary reads
-- `crown_reigns` on every profile load whose ledger window holds a
-- `chip_crown_taken` debit, and on `/crown`, and drained pods live for hours
-- after a deploy, so dropping the table in the same release fails those
-- calls in the old pod. Files in this folder are invisible to the migration
-- runner; build.rs warns on every build while any remain here.
--
-- The crown is gone (262_drop_crown.sql). Its reign history goes with it;
-- the `chip_crown_taken` ledger rows stay. The DELETE repeats 262's, for a
-- monthly `crown` badge a draining pod settled after 262 ran.
DELETE FROM profile_awards WHERE category = 'crown';

DROP TABLE crown_reigns;
