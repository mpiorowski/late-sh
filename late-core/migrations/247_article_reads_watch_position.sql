-- Where a reader left a News video, so the next watch resumes there: the
-- position (and the duration, for the card's `23:14 / 48:30`) mpv last
-- reported, on the `article_reads` row the watch already inserts, since a
-- watch is a read. NULL is never watched, watched to the end, or stopped
-- inside the first ten seconds: the next watch starts from the top. The
-- position is the one field on this table that moves; the row itself is
-- still only ever added, so reads merge as before.
ALTER TABLE article_reads
    ADD COLUMN watch_position_s BIGINT CHECK (watch_position_s >= 0),
    ADD COLUMN watch_duration_s BIGINT CHECK (watch_duration_s >= 0);
