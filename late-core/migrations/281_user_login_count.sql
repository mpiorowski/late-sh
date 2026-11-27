-- Logins per account, bumped once per ssh login (late-ssh/src/ssh.rs); a
-- drain handoff resuming on another pod is not a login. The "did you know"
-- splash line (late-ssh/src/app/common/splash_tips.rs) is chosen by it on
-- the first logins after the tour. Accounts older than the column have an
-- unknown count and are set past that series: it speaks to accounts it has
-- counted from their first login, not to regulars.
ALTER TABLE users ADD COLUMN login_count INTEGER NOT NULL DEFAULT 0;
UPDATE users SET login_count = 8;
