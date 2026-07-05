-- Green Dragon daily news: the village's "yesterday in Duskmere" feed
-- (LoGD's news.php / addnews). One row per item, keyed by the same UTC
-- day-number the game uses for its daily reset, so the view can page by day.
-- user_id is the item's subject when it has one (NULL = a system line);
-- deleting the user removes their news. Bodies are short pre-rendered text —
-- the game composes the prose before writing. Items expire after 180 days,
-- pruned opportunistically on write.
CREATE TABLE greendragon_news (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    day BIGINT NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL
);

CREATE INDEX greendragon_news_day_idx ON greendragon_news (day DESC, created DESC);
