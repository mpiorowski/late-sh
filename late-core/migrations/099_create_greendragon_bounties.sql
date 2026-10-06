-- Green Dragon bounty board (LoGD modules/dag.php): a price on a warrior's
-- head, collected by whoever wins the PvP kill. `setter` is NULL for
-- system-placed bounties; `winner` is NULL on a closed row when the house
-- collected instead (the target slew the dragon, or their character was
-- deleted). `set_at` carries the activation delay — the insert stamps up to
-- four hours into the future, and a bounty is invisible and uncollectable
-- until it matures. Closed rows are pruned after `expirecontent`/10 = 18
-- days, opportunistically on read (upstream sweeps from its admin page).
CREATE TABLE greendragon_bounties (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    target UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    setter UUID REFERENCES users(id) ON DELETE SET NULL,
    amount BIGINT NOT NULL,
    set_at TIMESTAMPTZ NOT NULL,
    open BOOLEAN NOT NULL DEFAULT true,
    winner UUID REFERENCES users(id) ON DELETE SET NULL,
    closed_at TIMESTAMPTZ
);

CREATE INDEX greendragon_bounties_open_target_idx
    ON greendragon_bounties (target) WHERE open;
