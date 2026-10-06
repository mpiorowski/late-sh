-- Pencil marks: one 9-bit candidate mask per cell, same 9x9 row-major shape as
-- grid/fixed_mask. Defaulted to an all-zero board so games saved before pencil
-- marks were persisted restore as "no notes" with no backfill, and so the
-- column can be NOT NULL from the start.
ALTER TABLE sudoku_games
    ADD COLUMN notes JSONB NOT NULL DEFAULT
        '[[0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0],
          [0,0,0,0,0,0,0,0,0]]'::jsonb;
