-- Sharing a News article pays chips, and deleting the article frees its URL
-- to be shared again, so the `articles` row cannot be the record of payment:
-- the `chip_ledger` row is. Every share looks up whether this user was ever
-- paid for this URL before crediting, and that lookup must not turn into a
-- sequential scan over a ledger that only grows.
CREATE INDEX chip_ledger_news_shared_idx
    ON chip_ledger (user_id, source_ref)
    WHERE reason = 'news_shared';
