-- The pet grows up, and every account has one. Days together (one per UTC
-- day its owner pressed a key with the pet out) carry it through drawn
-- stages, each entered by a pick its owner makes (the animal at the hatch,
-- then one of two drawings of a part); a treat a day earned by playing the
-- house's games names it for the day and feeds a bond; tricks are taught
-- over separate days. Nothing
-- here ever goes down. The closed sets are the enums in
-- `late-core/src/models/pet.rs`; the checks keep the rows honest.
ALTER TABLE pet_companions
    ADD COLUMN days_together INT NOT NULL DEFAULT 0 CHECK (days_together >= 0),
    ADD COLUMN last_day DATE,
    ADD COLUMN stage TEXT NOT NULL DEFAULT 'egg'
        CHECK (stage IN ('egg', 'baby', 'young', 'adult', 'elder')),
    -- The drawing picked on entering each stage after the hatch, in the
    -- order they are picked: the top (ears or crest) at young, the feet at
    -- adult, the face at elder. Read with `stage` as a `Form`.
    ADD COLUMN looks TEXT[] NOT NULL DEFAULT '{}'
        CHECK (looks <@ ARRAY['one', 'two']),
    ADD CONSTRAINT pet_companions_looks_fit_stage CHECK (
        cardinality(looks) = CASE stage
            WHEN 'young' THEN 1
            WHEN 'adult' THEN 2
            WHEN 'elder' THEN 3
            ELSE 0
        END
    ),
    ADD COLUMN bond_days INT NOT NULL DEFAULT 0 CHECK (bond_days >= 0),
    ADD COLUMN last_bond_day DATE,
    ADD COLUMN tricks TEXT[] NOT NULL DEFAULT '{}'
        CHECK (tricks <@ ARRAY['spin', 'bow', 'hop', 'dance']),
    ADD COLUMN learning TEXT
        CHECK (learning IS NULL OR learning IN ('spin', 'bow', 'hop', 'dance')),
    ADD COLUMN learning_days INT NOT NULL DEFAULT 0 CHECK (learning_days >= 0),
    ADD COLUMN last_practice DATE,
    ADD COLUMN visits_seen_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    -- Counts stored growth writes, so a session holding two copies of the
    -- row can tell which one is newer. Owned by `PetCompanion::store`.
    ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;

-- Every pet starts over as an egg: the defaults above are the reset, and
-- nothing seeds a row from its calendar age. The name and the last pat are
-- kept; what it is (cat, dog, bird) is picked again at the hatch. A row
-- counts its first day together on its owner's next key and hatches the
-- day after.

-- A treat: earned by the first win of a UTC day in any of the house's
-- games (or ten minutes in a door), waiting in the pouch until the owner
-- gives it. `kind` is the family of games it came from. The primary key is
-- the day's claim (one treat per UTC day, whatever the kind); the partial
-- unique index keeps at most one waiting.
CREATE TABLE pet_treats (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('puzzle', 'arcade', 'match', 'table', 'door')),
    earned_on DATE NOT NULL,
    given_on DATE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, earned_on)
);

CREATE UNIQUE INDEX pet_treats_one_waiting_idx
    ON pet_treats (user_id)
    WHERE given_on IS NULL;

-- Someone else at the pet: a pat from its profile, or a call by their own
-- pet. The primary key is the claim: once per visitor, pet, kind and UTC
-- day. A pet goes calling once a day, whoever it calls on.
CREATE TABLE pet_visits (
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visitor_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day DATE NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('pat', 'call')),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (owner_id, visitor_id, day, kind),
    CHECK (owner_id <> visitor_id)
);

CREATE INDEX pet_visits_owner_created_idx ON pet_visits (owner_id, created);

CREATE UNIQUE INDEX pet_visits_one_call_a_day_idx
    ON pet_visits (visitor_id, day)
    WHERE kind = 'call';

-- The pet is nobody's purchase any more: every account has an egg, so the
-- Shop stops selling the unlock. The row stays for the purchases that
-- point at it.
UPDATE marketplace_items
SET active = false,
    updated = current_timestamp
WHERE sku = 'pet_companion';
