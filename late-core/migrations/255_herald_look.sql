-- The herald's look (late-ssh `app/herald/look.rs`): what the model saw in
-- the day's picture, the alt text and the warning the post carries. It is
-- written with the picture in one statement, before the first network
-- call, so a retry posts the same mark and never asks the model again.
-- Rows picked before the look existed went out unreviewed and say so
-- (`"unseen"`, the herald's `Look::Unseen`). A picture never sits without
-- its look.
ALTER TABLE herald_posts ADD COLUMN look JSONB;
UPDATE herald_posts SET look = '"unseen"'::jsonb WHERE picture IS NOT NULL;
ALTER TABLE herald_posts ADD CONSTRAINT herald_posts_picture_has_look
    CHECK ((picture IS NULL) = (look IS NULL));
