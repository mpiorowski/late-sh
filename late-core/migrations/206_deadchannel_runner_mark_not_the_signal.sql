-- The Signal's `╬` is the paragon count behind the level in the badge
-- (`▚3╬2`, GAME.md "The mark"), so it left the mark alphabet
-- (`glyphs.rs::MARK_ALPHABET`) and `Look::parse` rejects a look wearing
-- it. The starter looks rolled before that drew from the whole glyph
-- alphabet; the ones that wear it are moved to the plain cross, the
-- nearest shape. Only those rows are written, so the change trigger
-- wakes the directory for them alone.
UPDATE deadchannel_runners
SET look = jsonb_set(look, '{mark,glyph}', '"┼"'),
    updated = current_timestamp
WHERE look -> 'mark' ->> 'glyph' = '╬';
