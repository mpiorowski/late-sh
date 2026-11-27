-- Treats grow the pet. The hatch still waits on days together; every stage
-- after it waits on treats eaten (`late-ssh/src/app/pet/state.rs`,
-- `need_for`). A treat is earned one a UTC day at most, so nothing is ground.
-- Written only by `PetCompanion::store`, like the rest of the growth.
ALTER TABLE pet_companions
    ADD COLUMN treats_eaten INT NOT NULL DEFAULT 0 CHECK (treats_eaten >= 0);

-- The count starts from the treats already given. `pet_treats` came with
-- the pets' reset in migration 289, so a given row is this pet's unless its
-- owner ran `/pet reset` since, which keeps the rows. An egg eats nothing,
-- so an egg with given rows is one of those and starts at zero. A pet reset
-- and hatched again since 289 keeps the few treats its earlier self ate:
-- nothing stores when a reset happened.
UPDATE pet_companions AS pet
SET treats_eaten = given.eaten
FROM (
    SELECT user_id, count(*)::INT AS eaten
    FROM pet_treats
    WHERE given_on IS NOT NULL
    GROUP BY user_id
) AS given
WHERE pet.user_id = given.user_id
  AND pet.stage <> 'egg';
