-- The two monthly earnings aggregates (`fetch_monthly_chip_earners` for the
-- Top Chips board, refreshed every 5 minutes, and the profile-award snapshot)
-- sum a whole month of `chip_ledger` grouped by user. Neither can use
-- `chip_ledger_positive_created_idx`: it is partial on `delta > 0` and both
-- sums include debits, so today they seq scan the entire table and get slower
-- with every row ever written rather than with the month they ask for.
--
-- A non-partial index on `created_at` bounds them to their own window, and
-- INCLUDE-ing the three columns they read makes the aggregate index-only.
-- That pays off here because the ledger is append-only apart from the user
-- cascade delete, so the visibility map stays set.
--
-- The partial index goes: no query filters on `delta > 0` (the only such
-- predicate in the codebase is the insert filter inside
-- `UserChips::restore_floor`), so it was maintained for nobody.
DROP INDEX chip_ledger_positive_created_idx;

CREATE INDEX chip_ledger_created_earnings_idx
    ON chip_ledger (created_at)
    INCLUDE (user_id, delta, reason);
