-- Every process-wide switch is gone: the paper, its Outside page, the
-- Artboard gallery and the job feed are always on, and first contact is
-- staff only by decision in code. Nothing reads or writes these rows.
DROP TRIGGER app_flags_changed ON app_flags;
DROP FUNCTION notify_app_flag_changed();
DROP TABLE app_flags;
