-- Author-shared translation rows: written by the author's "translate my
-- messages to English" opt-in and displayed to every viewer reading that
-- target language, unlike private rows created by a reader's `t`.
ALTER TABLE message_translations ADD COLUMN author_shared boolean NOT NULL DEFAULT false;
