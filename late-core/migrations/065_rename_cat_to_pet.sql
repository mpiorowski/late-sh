ALTER TABLE cat_companions RENAME TO pet_companions;

ALTER TABLE pet_companions
    ADD COLUMN species TEXT NOT NULL DEFAULT 'cat';

UPDATE marketplace_items
SET sku = 'pet_companion',
    name = 'Pet Companion',
    description = 'Unlock the sidebar pet, the care modal, and switch between cat and dog ASCII.',
    payload = '{"feature":"pet_companion"}'::jsonb,
    updated = current_timestamp
WHERE sku = 'cat_companion';
