//! What makes a daily the same for everyone: the stable hashes behind each
//! game's day seed and deal, and the rule that picks a pack's entry for a
//! date. Every saved board is rebuilt from these, so their output must never
//! change; `std`'s hashers make no such promise.

use std::{collections::HashMap, hash::Hash};

use chrono::NaiveDate;

/// FNV-1a, 64-bit, over the bytes of `text`.
pub fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// One splitmix64 step: the next value of a stream seeded with `value`,
/// spread over all 64 bits.
pub fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Fisher-Yates over a chain of splitmix64 steps from `seed`: the same
/// order for the same seed on every machine.
pub fn shuffle<T>(items: &mut [T], mut seed: u64) {
    for index in (1..items.len()).rev() {
        seed = splitmix64(seed);
        items.swap(index, (seed % (index as u64 + 1)) as usize);
    }
}

/// The entry of `pack` to claim for a day. An entry never claimed comes
/// first, picked by the FNV-1a hash of `seed` (the caller's day text, which
/// names the game and a version); once every entry has had a day, the one
/// claimed longest ago (ties by key) comes round again. `last_used` maps an
/// entry's key to the last day it was claimed. `None` only for an empty pack.
pub fn choose_for_date<'a, T, K>(
    pack: &'a [T],
    key: impl Fn(&T) -> &K,
    last_used: &HashMap<K, NaiveDate>,
    seed: &str,
) -> Option<&'a T>
where
    K: Eq + Hash + Ord,
{
    let unused: Vec<&T> = pack
        .iter()
        .filter(|entry| !last_used.contains_key(key(entry)))
        .collect();
    if !unused.is_empty() {
        return Some(unused[(fnv1a(seed) % unused.len() as u64) as usize]);
    }
    pack.iter().min_by(|a, b| {
        last_used[key(a)]
            .cmp(&last_used[key(b)])
            .then_with(|| key(a).cmp(key(b)))
    })
}
