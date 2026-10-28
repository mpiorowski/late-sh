//! Builds the Word Wheel pack from SCOWL (wordlist.aspell.net, the
//! `scowl-2020.12.07` release), `en_US` up to size 60:
//!
//! ```sh
//! tar xzf scowl-2020.12.07.tar.gz
//! cargo run -p late-core --bin gen_word_wheel -- scowl-2020.12.07/final
//! ```
//!
//! Writes `late-ssh/assets/word_wheel/wheels.json`, one wheel per line. The
//! word list keeps lowercase `a`-`z` words of `MIN_WORD_LEN` letters or more
//! with no `s` (plurals would pad every wheel) and at most seven distinct
//! letters, less `BLOCKED`. Every set of seven letters some word uses in
//! full is a candidate wheel; a hub is good when the wheel then accepts
//! `MIN_ANSWERS` to `MAX_ANSWERS` words. Each candidate keeps the good hub
//! whose wheel id hashes lowest under `SEED`, and the pack keeps the
//! `PACK_SIZE` candidates that hash lowest, so the pick does not depend on
//! the list's order and two wheels never share their letters.

use std::{collections::HashMap, env, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use late_core::{
    daily::{fnv1a, splitmix64},
    word_wheel::{MIN_WORD_LEN, WHEEL_LETTERS, WordWheel, fits},
};

const PACK_SIZE: usize = 2000;
const SEED: u64 = 0x6c61_7465_2d77_6865;
const MIN_ANSWERS: usize = 25;
const MAX_ANSWERS: usize = 70;
const SIZES: [u8; 7] = [10, 20, 35, 40, 50, 55, 60];
const LISTS: [&str; 2] = ["english-words", "american-words"];
/// Words SCOWL lists that a daily puzzle should not ask anyone to type:
/// slurs, and the vulgar and sexual words a family newspaper would leave
/// out. Matched whole, so `cocktail` and `grape` stay.
const BLOCKED: &[&str] = &[
    "anal",
    "anally",
    "beaner",
    "bitch",
    "bitched",
    "bitchier",
    "bitchily",
    "bitching",
    "bitchy",
    "boner",
    "boob",
    "boobed",
    "boobing",
    "booby",
    "bugger",
    "buggered",
    "buggering",
    "buggery",
    "chinaman",
    "chink",
    "chinked",
    "chinking",
    "clit",
    "clitoral",
    "cock",
    "coolie",
    "coon",
    "crap",
    "crapped",
    "crapper",
    "crappier",
    "crapping",
    "crappy",
    "cumming",
    "cunt",
    "dago",
    "damn",
    "damned",
    "darkie",
    "darky",
    "dick",
    "dickhead",
    "dildo",
    "dyke",
    "ejaculate",
    "fagged",
    "fagging",
    "faggot",
    "fagot",
    "fagoting",
    "fart",
    "farted",
    "farting",
    "fuck",
    "fucked",
    "fucker",
    "fucking",
    "gimp",
    "gimped",
    "gimping",
    "gimpy",
    "goddamn",
    "goddamned",
    "golliwog",
    "gook",
    "gypped",
    "gypper",
    "gypping",
    "homo",
    "honkie",
    "honky",
    "hooker",
    "hornier",
    "horny",
    "injun",
    "jewboy",
    "kaffir",
    "kike",
    "kraut",
    "midget",
    "mongol",
    "mongoloid",
    "mulatto",
    "negro",
    "negroid",
    "nigga",
    "niggard",
    "niggaz",
    "nigger",
    "nympho",
    "octoroon",
    "orgy",
    "pecker",
    "penile",
    "phalli",
    "phallic",
    "pimp",
    "pimped",
    "pimping",
    "poofter",
    "poop",
    "porn",
    "porno",
    "prick",
    "queer",
    "queered",
    "queerer",
    "queering",
    "queerly",
    "rape",
    "raped",
    "raper",
    "raping",
    "redneck",
    "retard",
    "retarded",
    "titty",
    "turd",
    "twat",
    "vagina",
    "vaginae",
    "vaginal",
    "vaginally",
    "wank",
    "wanked",
    "wanker",
    "wanking",
    "wetback",
    "whore",
    "whoring",
];

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(scowl_dir) = args.next() else {
        bail!("usage: gen_word_wheel <scowl final dir> [output dir]");
    };
    let output_dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "late-ssh/assets/word_wheel".to_string()),
    );

    let mut by_letters: HashMap<u32, Vec<String>> = HashMap::new();
    let mut word_count = 0usize;
    for list in LISTS {
        for size in SIZES {
            let path = PathBuf::from(&scowl_dir).join(format!("{list}.{size}"));
            // SCOWL ships Latin-1; every word kept is ASCII, so the bytes
            // are read as they are.
            let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            for line in bytes.split(|byte| *byte == b'\n') {
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                let Some(word) = keepable(line) else {
                    continue;
                };
                let bucket = by_letters.entry(letter_mask(&word)).or_default();
                if !bucket.contains(&word) {
                    bucket.push(word);
                    word_count += 1;
                }
            }
        }
    }

    let mut candidates: Vec<(u64, WordWheel)> = by_letters
        .keys()
        .filter(|mask| mask.count_ones() as usize == WHEEL_LETTERS)
        .filter_map(|&mask| best_wheel(&by_letters, mask))
        .collect();
    let candidate_count = candidates.len();
    if candidate_count < PACK_SIZE {
        bail!("only {candidate_count} candidate wheels, wanted {PACK_SIZE}");
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.letters.cmp(&b.1.letters)));
    candidates.truncate(PACK_SIZE);
    let mut wheels: Vec<WordWheel> = candidates.into_iter().map(|(_, wheel)| wheel).collect();
    wheels.sort_by(|a, b| a.letters.cmp(&b.letters));

    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating {}", output_dir.display()))?;
    let lines = wheels
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?;
    let path = output_dir.join("wheels.json");
    fs::write(&path, format!("[\n{}\n]\n", lines.join(",\n")))
        .with_context(|| format!("writing {}", path.display()))?;
    let answers: usize = wheels.iter().map(|wheel| wheel.answers.len()).sum();
    println!(
        "{}: {} wheels from {candidate_count} candidates, {word_count} words, {:.1} answers a wheel",
        path.display(),
        wheels.len(),
        answers as f64 / wheels.len() as f64
    );
    Ok(())
}

/// The word on a list line, when the pack can use it.
fn keepable(line: &[u8]) -> Option<String> {
    if line.len() < MIN_WORD_LEN || !line.iter().all(u8::is_ascii_lowercase) || line.contains(&b's')
    {
        return None;
    }
    let word = String::from_utf8(line.to_vec()).ok()?;
    if letter_mask(&word).count_ones() as usize > WHEEL_LETTERS || BLOCKED.contains(&word.as_str())
    {
        return None;
    }
    Some(word)
}

fn letter_mask(word: &str) -> u32 {
    word.bytes().fold(0, |mask, byte| mask | 1 << (byte - b'a'))
}

/// The seven letters of `mask` with their good hub that hashes lowest, and
/// that hash, or `None` when no hub gives a wheel of the right size.
fn best_wheel(by_letters: &HashMap<u32, Vec<String>>, mask: u32) -> Option<(u64, WordWheel)> {
    let letters: Vec<char> = (0..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| char::from(b'a' + bit))
        .collect();
    let mut words: Vec<&String> = Vec::new();
    let mut submask = mask;
    loop {
        if let Some(bucket) = by_letters.get(&submask) {
            words.extend(bucket);
        }
        if submask == 0 {
            break;
        }
        submask = (submask - 1) & mask;
    }

    letters
        .iter()
        .filter_map(|&hub| {
            let id: String = std::iter::once(hub)
                .chain(letters.iter().copied().filter(|&letter| letter != hub))
                .collect();
            let mut answers: Vec<String> = words
                .iter()
                .filter(|word| fits(&id, word))
                .map(|word| (*word).clone())
                .collect();
            if !(MIN_ANSWERS..=MAX_ANSWERS).contains(&answers.len()) {
                return None;
            }
            answers.sort();
            let key = splitmix64(fnv1a(&id) ^ SEED);
            Some((
                key,
                WordWheel {
                    letters: id,
                    answers,
                },
            ))
        })
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.letters.cmp(&b.1.letters)))
}
