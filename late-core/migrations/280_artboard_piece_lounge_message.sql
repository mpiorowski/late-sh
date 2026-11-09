ALTER TABLE artboard_pieces
    ADD COLUMN lounge_message_id UUID REFERENCES chat_messages(id) ON DELETE SET NULL;

CREATE UNIQUE INDEX artboard_pieces_lounge_message_id_key
    ON artboard_pieces (lounge_message_id)
    WHERE lounge_message_id IS NOT NULL;

CREATE OR REPLACE FUNCTION notify_artboard_pieces_changed() RETURNS trigger AS $fn$
BEGIN
    PERFORM pg_notify('artboard_pieces_changed', '');
    RETURN NULL;
END;
$fn$ LANGUAGE plpgsql;

CREATE TRIGGER artboard_pieces_changed
    AFTER INSERT OR UPDATE OR DELETE ON artboard_pieces
    FOR EACH STATEMENT EXECUTE FUNCTION notify_artboard_pieces_changed();

CREATE TRIGGER artboard_piece_votes_changed
    AFTER INSERT OR DELETE ON artboard_piece_votes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_artboard_pieces_changed();
