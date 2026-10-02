-- Invites: each account's one stable invite code, the one referral row an
-- invitee can carry, and the newcomer activity that referral is judged on.
-- Contracts live in late-ssh/src/app/referral/CONTEXT.md.

CREATE TABLE invite_codes (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    -- 8 symbols of the link-code alphabet, lowercased: no 0/1/i/o.
    code TEXT NOT NULL UNIQUE CHECK (code ~ '^[2-9a-hj-np-z]{8}$'),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

CREATE TABLE referrals (
    -- One inviter per account, ever: the primary key is the invitee.
    invitee_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    inviter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (source IN ('ssh', 'settings')),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'qualified', 'paid', 'expired')),
    -- The end of the invitee's judging window. Activity after it never
    -- counts, and a still-pending row past it expires.
    judged_until TIMESTAMPTZ NOT NULL,
    qualified_at TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    CHECK (inviter_id <> invitee_id),
    CHECK ((status IN ('qualified', 'paid')) = (qualified_at IS NOT NULL)),
    CHECK ((status = 'paid') = (paid_at IS NOT NULL))
);

-- The inviter's list in Settings, and the monthly paid count.
CREATE INDEX idx_referrals_inviter ON referrals (inviter_id, created DESC);
-- The sweeper's two queues.
CREATE INDEX idx_referrals_open ON referrals (status, created)
    WHERE status IN ('pending', 'qualified');

CREATE TABLE newcomer_activity_days (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day DATE NOT NULL,
    -- Wall-clock UTC minutes with keyboard input, counted once per minute
    -- however many sessions the account has open.
    active_minutes INT NOT NULL CHECK (active_minutes BETWEEN 1 AND 1440),
    -- The latest counted minute: a second session reporting the same minute
    -- loses the `last_minute < EXCLUDED.last_minute` guard and counts nothing.
    last_minute TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, day)
);
