-- The work card's closed fields, and the tags the job matcher joins on.
--
-- `work_type` was free text ("contract, full-time, freelance"). It becomes
-- one of five values so the editor cycles it and a job match can compare
-- it. Existing text is folded onto the closest value; anything that names
-- none of them reads as 'any', which is what a person who typed three
-- kinds at once meant.
UPDATE work_profiles SET work_type = CASE
    WHEN work_type ILIKE '%full%' THEN 'full-time'
    WHEN work_type ILIKE '%contract%' THEN 'contract'
    WHEN work_type ILIKE '%freelanc%' THEN 'freelance'
    WHEN work_type ILIKE '%part%' THEN 'part-time'
    ELSE 'any'
END;
ALTER TABLE work_profiles DROP CONSTRAINT work_profiles_work_type_check;
ALTER TABLE work_profiles ADD CONSTRAINT work_profiles_work_type_check
    CHECK (work_type IN ('full-time', 'contract', 'freelance', 'part-time', 'any'));

-- `skills` stays as written, for display. `skills_tags` is the same list
-- run through the tag vocabulary (late-ssh/src/app/jobs/vocab.rs), which
-- the job matcher joins on. Cards saved before this column carry their raw
-- skills here; the editor renormalizes on the next save.
ALTER TABLE work_profiles ADD COLUMN skills_tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]
    CHECK (cardinality(skills_tags) <= 12);
UPDATE work_profiles SET skills_tags = skills;
