-- One row per month whose award roll was posted to #lounge. The unique month
-- is the claim: every replica's snapshot loop tries to insert it after its
-- pass, and only the one that wins posts the roll.
CREATE TABLE profile_award_announcements (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    period_month DATE NOT NULL UNIQUE
);
