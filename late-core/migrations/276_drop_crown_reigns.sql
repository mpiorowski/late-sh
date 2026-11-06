-- The crown is gone (262_drop_crown.sql). Its reign history goes with it;
-- the `chip_crown_taken` ledger rows stay. The DELETE repeats 262's, for a
-- monthly `crown` badge a draining pod settled after 262 ran.
DELETE FROM profile_awards WHERE category = 'crown';

DROP TABLE crown_reigns;
