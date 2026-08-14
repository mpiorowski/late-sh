-- A durable, tamper-resistant record that an account graduated BashQuest
-- (completed every level, via the bashquest door). Written only by late-ssh,
-- and only after late-bashquest independently confirms bashquest.sh wrote a
-- graduation certificate on its own PVC for that session's account -- the
-- player never gets shell access to that filesystem, so this table can only
-- ever grow from an actual completion, never a self-reported claim (see
-- late-ssh/src/app/door/bashquest/CONTEXT.md). Read-only published to a
-- public GitHub Pages gallery by a scheduled job outside this repo.
--
-- user_id is ON DELETE SET NULL, not CASCADE, same reasoning as
-- arcade_handles: the certificate is a historical fact that should outlive
-- account deletion, so handle/certificate text are captured directly here
-- rather than joined.
CREATE TABLE bashquest_graduates (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID UNIQUE REFERENCES users(id) ON DELETE SET NULL,
    handle TEXT NOT NULL,
    certificate TEXT NOT NULL,
    certificate_digest TEXT NOT NULL
);
