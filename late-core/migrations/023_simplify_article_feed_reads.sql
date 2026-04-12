ALTER TABLE article_feed_reads
ADD COLUMN last_read_at TIMESTAMPTZ;

UPDATE article_feed_reads
SET last_read_at = last_read_created
WHERE last_read_created IS NOT NULL;

ALTER TABLE article_feed_reads
DROP CONSTRAINT article_feed_reads_checkpoint_chk;

ALTER TABLE article_feed_reads
DROP COLUMN last_read_article_id,
DROP COLUMN last_read_created;
