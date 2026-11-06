-- The Late Edition's YESTERDAY ON LATE.SH column (late-ssh `app/paper`):
-- the covered day in numbers, from late.sh's own rows and no model call
-- (the games finished on each shelf, the radio's top stations by listener,
-- the tracks queued in the booth), printed first after the operator's
-- word. One more section kind, same claim shape as the other two.
ALTER TABLE paper_sections DROP CONSTRAINT paper_sections_section_check;
ALTER TABLE paper_sections ADD CONSTRAINT paper_sections_section_check
    CHECK (section IN ('reading', 'outside', 'yesterday'));
