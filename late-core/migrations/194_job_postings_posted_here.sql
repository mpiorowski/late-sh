-- Postings made on late.sh itself (late-ssh `app/jobs`, the post form on
-- the Jobs shelf): a fourth source, `late`, whose rows are written by a
-- person rather than pulled from a feed, and go active the moment they
-- are saved. `posted_by` is who wrote it, for the take-down (theirs, or a
-- moderator's) and the per-person cap; a feed row has none.
ALTER TABLE job_postings DROP CONSTRAINT job_postings_source_check;
ALTER TABLE job_postings ADD CONSTRAINT job_postings_source_check
    CHECK (source IN ('hn', 'wwr', 'jobicy', 'late'));
ALTER TABLE job_postings ADD COLUMN posted_by UUID;
ALTER TABLE job_postings ADD CONSTRAINT job_postings_posted_by_check
    CHECK ((source = 'late') = (posted_by IS NOT NULL));
CREATE INDEX job_postings_posted_by_idx ON job_postings (posted_by)
    WHERE status = 'active' AND posted_by IS NOT NULL;
