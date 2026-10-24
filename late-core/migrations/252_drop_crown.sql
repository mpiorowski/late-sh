-- The crown is gone: no `/crown`, no glyphs, no monthly `crown` badge.
--
-- Its badges go with it, since nothing can name or render the category any
-- more. The `chip_crown_taken` ledger rows stay: the ledger has to keep
-- summing to each balance, and the reason stays retired in `ChipMove` so
-- those debits remain off the Top Chips board.
DELETE FROM profile_awards WHERE category = 'crown';

DROP TABLE crown_reigns;
