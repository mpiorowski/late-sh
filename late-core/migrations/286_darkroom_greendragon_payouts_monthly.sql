-- Green Dragon and A Dark Room join the door milestones' monthly rule: each
-- milestone pays at most once every 30 days per account, on top of the gate
-- each already had (one kill of one character, one run). A Dark Room's two
-- endings are two milestones with a window each, so a plain escape and a
-- beacon escape inside the same month both pay. The amounts stay as they are.
--
-- Claims already banked carry period_kind = 'event', which the window does not
-- read, so the first payout after this migration lands on an open window.

UPDATE reward_templates
SET claim_policy = 'cooldown',
    cooldown_seconds = 2592000,
    description = 'Grow strong enough to face the Green Dragon in its forest lair and slay it. Pays once per kill, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'greendragon_dragon_slain';

UPDATE reward_templates
SET claim_policy = 'cooldown',
    cooldown_seconds = 2592000,
    description = 'Light the fire, raise the village, cross the wasteland, and fly the wrecked starship out through the debris cloud. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'darkroom_escape';

UPDATE reward_templates
SET claim_policy = 'cooldown',
    cooldown_seconds = 2592000,
    description = 'Clear the ravaged battleship, kill the immortal wanderer, take the fleet beacon, and fly out holding it. Pays once per run, and again 30 days after the last time it paid.',
    updated = current_timestamp
WHERE key = 'darkroom_beacon_escape';
