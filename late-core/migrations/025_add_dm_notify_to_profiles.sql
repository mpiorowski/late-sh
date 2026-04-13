ALTER TABLE profiles ADD COLUMN dm_notify TEXT NOT NULL DEFAULT 'unfocused'
  CHECK (dm_notify IN ('unfocused', 'always', 'off'));

ALTER TABLE profiles ADD COLUMN dm_notify_cooldown_mins INT NOT NULL DEFAULT 5
  CHECK (dm_notify_cooldown_mins > 0);
