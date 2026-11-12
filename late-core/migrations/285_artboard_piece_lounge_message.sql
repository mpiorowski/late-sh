-- A hung piece's card in #lounge: the chat message that stands for it.
-- Deleting the message (its author's `d`, a moderator) leaves the piece up
-- without a card.
ALTER TABLE artboard_pieces
    ADD COLUMN lounge_message_id UUID REFERENCES chat_messages(id) ON DELETE SET NULL;

CREATE UNIQUE INDEX artboard_pieces_lounge_message_id_key
    ON artboard_pieces (lounge_message_id)
    WHERE lounge_message_id IS NOT NULL;

-- Every replica keeps the cards in memory (`GalleryService::lounge_cards`)
-- and re-reads them on this signal. It fires on what a card shows or
-- whether it exists: a card linked or unlinked, a piece taken down, a hand
-- of applause given or taken back. Splash and feature stamps change
-- nothing a card draws, so they stay quiet.
CREATE FUNCTION notify_artboard_pieces_changed() RETURNS trigger AS $fn$
BEGIN
    PERFORM pg_notify('artboard_pieces_changed', '');
    RETURN NULL;
END;
$fn$ LANGUAGE plpgsql;

CREATE TRIGGER artboard_pieces_changed
    AFTER UPDATE OF lounge_message_id, removed_at ON artboard_pieces
    FOR EACH STATEMENT EXECUTE FUNCTION notify_artboard_pieces_changed();

CREATE TRIGGER artboard_piece_votes_changed
    AFTER INSERT OR DELETE ON artboard_piece_votes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_artboard_pieces_changed();
