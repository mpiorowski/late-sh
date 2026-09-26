-- Local development only. Replaces today's two edition-level sections;
-- room pages, chat messages, user settings, and older editions are untouched.
-- Invoke through seed_paper_test_data.sh, which validates paper_paragraphs.
\set ON_ERROR_STOP on

BEGIN;

INSERT INTO paper_sections (edition, section, status, text, attempts, generated_at)
VALUES (
    (current_timestamp AT TIME ZONE 'UTC')::date,
    'reading',
    'ready',
    E'[TEST EDITION - synthetic local data]\n\n'
        || 'The clubhouse librarian found a manual explaining that every scrollbar '
        || 'deserves a proper workout. This edition has numbered paragraphs, long '
        || E'wrapped lines, blank lines, and wide characters.\n\n'
        || 'Try the wheel, j/k, PageUp/PageDown, Home, track clicks, and dragging '
        || 'the thumb. Resize the terminal, then use [x] to close and /paper to reopen.',
    1,
    current_timestamp
), (
    (current_timestamp AT TIME ZONE 'UTC')::date,
    'outside',
    'ready',
    E'[TEST EDITION - dispatches from an imaginary town]\n\n' || (
        SELECT string_agg(
            format('[%s/%s] %s', n, :'paper_paragraphs',
                (ARRAY[
                    'The midnight tram arrived exactly on time. Its only passenger was a gardener carrying a small bonsai, a large thermos, and a timetable annotated with increasingly optimistic estimates.',
                    'The library opened a quiet room for noisy keyboards. Visitors agreed that the best book was the one they finally finished, and that the second-best book was still waiting on the bedside table.',
                    'The cafe has added a terminal beside the window. Today''s chalkboard reads: cafe, café, 東京, 界界. Long lines should wrap cleanly while the scrollbar stays in its own narrow margin.',
                    'The observatory reports clear skies and a fine view of the moon. Nobody has discovered a new constellation, but several people have renamed the familiar ones after their pets.',
                    'The local newspaper has reached another numbered paragraph. Keep scrolling toward the final marker; the last page should stay filled with text when the wheel or thumb reaches the bottom.'
                ])[1 + (n - 1) % 5]),
            E'\n\n' ORDER BY n
        )
        FROM generate_series(1, :'paper_paragraphs'::integer) AS n
    ) || E'\n\n[END OF TEST EDITION]',
    1,
    current_timestamp
)
ON CONFLICT (edition, section) DO UPDATE SET
    status = EXCLUDED.status,
    text = EXCLUDED.text,
    attempts = EXCLUDED.attempts,
    claimed_at = current_timestamp,
    generated_at = EXCLUDED.generated_at;

UPDATE app_flags SET enabled = true, updated = current_timestamp
WHERE key = 'paper_enabled' AND NOT enabled;

COMMIT;

SELECT edition, section, status, length(text) AS characters
FROM paper_sections
WHERE edition = (current_timestamp AT TIME ZONE 'UTC')::date
ORDER BY section;
