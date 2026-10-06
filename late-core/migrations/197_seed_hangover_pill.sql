-- The hangover pill: the bar's way back out. Buying one zeroes the buyer's
-- `user_drinks.drunk_points` in the purchase transaction, so their typing is
-- sober on the next message and the drunk tint clears. It is refused
-- uncharged while the buyer is already sober, so it can never take chips for
-- nothing.
--
-- Its own `item_kind = 'bar_consumable'`, not 'chat_consumable': every chat
-- consumable must target a room (`activate_chat_consumable_in_tx` fails the
-- purchase otherwise), and this one targets the buyer. It still lists on the
-- Chat tab, under Consumables, below the room effects.
INSERT INTO marketplace_items
    (sku, item_kind, slot, name, description, price_chips, payload, active, sort_order)
VALUES
    (
        'hangover_pill',
        'bar_consumable',
        NULL,
        'Hangover Pill',
        'Sober up at once: your typing comes back straight and the drunk tint clears.',
        250,
        '{}'::jsonb,
        true,
        4100
    );
