-- What @bartender remembers about a patron. Two kinds of row:
--
-- An exchange memory, kept on his own: one per order he answered (the
-- patron's order, his reply, and up to five more lines the patron said in
-- that room until five seconds after the reply landed).
--
-- A requested memory: one per "@bartender remember ...", holding just what
-- the patron asked him to remember. It is never an order, so it has no
-- reply and no trailing lines.
--
-- The words are copied, not referenced: a memory lives by its own rules,
-- whatever happens to the chat log it came from. Only three things end
-- one: the cap below, the patron saying "forget me", and the account
-- going away.
--
-- A patron keeps at most sixteen. The eight most recent requested memories
-- are kept first; every other slot goes to the newest of the rest, so a
-- patron who asks him to remember everything still has room for new
-- nights. `models/bartender_memory.rs` holds the rule.
CREATE TABLE bartender_memories (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The patron's chat message id (the order, or the remember request),
    -- kept only so one is stored once. Deliberately not a foreign key.
    order_message_id UUID NOT NULL UNIQUE,
    -- That message's own `created`: what eviction and the reset below
    -- compare.
    ordered_at TIMESTAMPTZ NOT NULL,
    -- The order's whole body, or for a requested memory just what to
    -- remember.
    order_body TEXT NOT NULL,
    requested BOOLEAN NOT NULL,
    replied_at TIMESTAMPTZ,
    reply_body TEXT,
    trailing_bodies TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    CONSTRAINT bartender_memories_reply_iff_exchange CHECK (
        (requested AND replied_at IS NULL AND reply_body IS NULL
                   AND cardinality(trailing_bodies) = 0)
        OR (NOT requested AND replied_at IS NOT NULL AND reply_body IS NOT NULL)
    )
);

CREATE INDEX bartender_memories_by_user
    ON bartender_memories (user_id, ordered_at DESC);

-- When a patron last said "forget me". No memory whose message is at or
-- before it may be stored, so one still in flight when they said it (the
-- model call, the five-second tail) cannot land after the wipe.
CREATE TABLE bartender_memory_resets (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    reset_at TIMESTAMPTZ NOT NULL
);
