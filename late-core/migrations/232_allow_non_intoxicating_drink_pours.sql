-- Every taken drink is logged, including non-intoxicating drinks whose buzz
-- is zero. They count on a bar's tab board without adding Top Drinkers points.
ALTER TABLE drink_pours DROP CONSTRAINT drink_pours_points_check;
ALTER TABLE drink_pours ADD CONSTRAINT drink_pours_points_check CHECK (points >= 0);
