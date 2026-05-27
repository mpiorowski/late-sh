UPDATE users
SET settings = settings - 'show_dashboard_wire'
WHERE settings ? 'show_dashboard_wire';
