-- The "Land on Home page" toggle became a choice of landing page
-- (clubhouse, home, zen). Accounts that had turned the toggle on keep
-- landing on Home; everyone else falls back to the Clubhouse default.
UPDATE users
SET settings = (settings - 'land_on_home')
        || CASE
               WHEN settings->'land_on_home' = 'true'::jsonb
                   THEN jsonb_build_object('landing_page', 'home')
               ELSE '{}'::jsonb
           END,
    updated = current_timestamp
WHERE settings ? 'land_on_home';
