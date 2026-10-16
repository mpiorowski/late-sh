-- The crossing (GAME.md, "The crossing pass"): one runner meets another
-- on the road and fights them while they sleep, and the answer the
-- sleeper is owed. One row per fight between two runners.
--
-- `attacker` is who started it: the one who stepped onto the crossing,
-- or, for an answer, the sleeper answering. `stake` is the defender's
-- bits on hand when the fight started, the most the attacker can take;
-- `bits_moved` is what changed hands when it ended, from the attacker's
-- side (what they took, or less what they lost). `answer_to` names the
-- crossing an answer settles; a crossing has none.
--
-- The three indexes are the rules: a sleeper is crossed at most once a
-- day, two runners cross once a day, and a crossing is answered once.
-- They are partial so an answer counts against none of the day's caps:
-- the answer is the sleeper's, whatever else the day held.
CREATE TABLE deadchannel_ambushes (
    id UUID PRIMARY KEY,
    day DATE NOT NULL,
    attacker_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    defender_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    attacker_username TEXT NOT NULL,
    defender_username TEXT NOT NULL,
    step INT CHECK (step IS NULL OR step > 0),
    stake BIGINT NOT NULL CHECK (stake >= 0),
    answer_to UUID REFERENCES deadchannel_ambushes(id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    outcome TEXT CHECK (outcome IN ('won', 'lost', 'ran', 'dropped')),
    bits_moved BIGINT NOT NULL DEFAULT 0,
    CHECK ((ended_at IS NULL) = (outcome IS NULL)),
    CHECK (attacker_user_id <> defender_user_id),
    CHECK ((answer_to IS NULL) <> (step IS NULL))
);

CREATE UNIQUE INDEX deadchannel_ambushes_one_a_day
    ON deadchannel_ambushes (defender_user_id, day) WHERE answer_to IS NULL;

CREATE UNIQUE INDEX deadchannel_ambushes_one_per_pair
    ON deadchannel_ambushes (
        LEAST(attacker_user_id, defender_user_id),
        GREATEST(attacker_user_id, defender_user_id),
        day
    ) WHERE answer_to IS NULL;

CREATE UNIQUE INDEX deadchannel_ambushes_one_answer
    ON deadchannel_ambushes (answer_to) WHERE answer_to IS NOT NULL;

-- What the sleeper's row reads: the fights on them, latest first.
CREATE INDEX deadchannel_ambushes_on_defender
    ON deadchannel_ambushes (defender_user_id, ended_at DESC);
