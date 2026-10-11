-- Every process-wide switch is gone (v0.48.4): the paper, its Outside page,
-- the Artboard gallery and the job feed are always on, and first contact is
-- staff only by a rule in code. Nothing reads or writes these rows. The
-- drop waited a release so a pod draining on the build that still read the
-- table, and a rollback to it, kept finding it.
DROP TRIGGER app_flags_changed ON app_flags;
DROP FUNCTION notify_app_flag_changed();
DROP TABLE app_flags;
