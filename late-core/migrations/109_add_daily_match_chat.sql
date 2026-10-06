-- Each claimed daily match gets a private two-player chat room (and an
-- enabled voice channel targeting it), created in the claim transaction.
-- Nullable: challenges have nobody to talk to, and matches claimed before
-- this feature simply have no chat. ON DELETE SET NULL so the 30-day chat
-- cleanup sweep can drop old rooms without touching match history.
ALTER TABLE daily_matches
    ADD COLUMN chat_room_id UUID REFERENCES chat_rooms(id) ON DELETE SET NULL;
