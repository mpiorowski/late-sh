-- A track sits in the Booth playlist once. Duplicate submissions made the
-- queue read as the same video on a loop, so the same (media_kind,
-- external_id) may not be queued or playing at the same time. A finished
-- track leaves the active set and can be requeued from History.

-- Retire the duplicates that predate the index: keep the playing row (or the
-- oldest queued one) per track and fail the rest.
WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY media_kind, external_id
               ORDER BY CASE status WHEN 'playing' THEN 0 ELSE 1 END, created
           ) AS position
    FROM media_queue_items
    WHERE status IN ('queued', 'playing')
)
UPDATE media_queue_items
SET status = 'failed',
    error = 'duplicate of a track already in the queue',
    ended_at = current_timestamp,
    updated = current_timestamp
WHERE id IN (SELECT id FROM ranked WHERE position > 1);

CREATE UNIQUE INDEX idx_media_queue_active_track
    ON media_queue_items (media_kind, external_id)
    WHERE status IN ('queued', 'playing');
