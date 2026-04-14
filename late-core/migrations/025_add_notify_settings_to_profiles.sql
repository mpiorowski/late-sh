ALTER TABLE profiles ADD COLUMN notify_kinds TEXT[] NOT NULL DEFAULT '{}';

ALTER TABLE profiles ADD COLUMN notify_cooldown_mins INT NOT NULL DEFAULT 0
  CHECK (notify_cooldown_mins >= 0);
