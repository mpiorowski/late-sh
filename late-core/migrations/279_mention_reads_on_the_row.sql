-- A mention's own read_at becomes the only read signal: opening the Mentions
-- entry stamps the rows instead of moving the mention_feed_reads watermark.
-- Everything the watermark had already cleared gets its stamp here, or the
-- unread counts would jump back up. The stamp is the watermark's own time,
-- which is when those mentions were read.
UPDATE notifications n
SET read_at = mfr.last_read_at,
    updated = current_timestamp
FROM mention_feed_reads mfr
WHERE n.user_id = mfr.user_id
  AND n.read_at IS NULL
  AND n.created <= mfr.last_read_at;

-- mention_feed_reads stays for now: a replica still on the old code reads it
-- until the rollout finishes, so it goes in a later migration. That migration
-- repeats the UPDATE above first, for what old sessions read after this ran.
