-- A cached verdict that the message is already written in the target
-- language: the model call happened, nothing needs rendering, and nobody
-- should pay for that call again. Rows with same_language keep the source
-- text in body so the row still describes exactly what was judged.
ALTER TABLE message_translations
    ADD COLUMN same_language boolean NOT NULL DEFAULT false;
