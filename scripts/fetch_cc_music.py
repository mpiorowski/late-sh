#!/usr/bin/env python3
"""
Download CC0/CC-BY music for late.sh radio and generate .m3u playlists.

Sources:
  Lofi:      HoliznaCC0 (CC0) via Bandcamp
  Ambient:   Curated FMA ambient set (CC-BY 4.0)
  Classical: Musopen (Public Domain) via Internet Archive
  Jazz:      Kevin MacLeod (CC-BY) via Internet Archive + HoliznaCC0 (CC0) via Bandcamp

Dependencies: yt-dlp, ffmpeg, python3
Usage: python3 scripts/fetch_cc_music.py [--genre lofi|ambient|classic|jazz|all] [--music-dir PATH] [--skip-m3u]
"""

import subprocess, json, os, sys, re, urllib.request, glob, argparse
from pathlib import Path

DEFAULT_MUSIC_DIR = Path(__file__).resolve().parent.parent / "music"
DEFAULT_LIQUIDSOAP_DIR = Path(__file__).resolve().parent.parent / "infra" / "liquidsoap"
MUSIC_DIR = DEFAULT_MUSIC_DIR
LIQUIDSOAP_DIR = DEFAULT_LIQUIDSOAP_DIR

# ---------------------------------------------------------------------------
# Source definitions
# ---------------------------------------------------------------------------

BANDCAMP_ALBUMS = {
    "lofi": [
        "https://holiznacc0.bandcamp.com/album/lofi-and-chill",
        "https://holiznacc0.bandcamp.com/album/public-domain-lo-fi",
        "https://holiznacc0.bandcamp.com/album/winter-lo-fi-2",
        "https://holiznacc0.bandcamp.com/album/city-slacker",
    ],
    "jazz": [
        "https://holiznacc0.bandcamp.com/album/lofi-jazz-guitar",
        "https://kevinmacleod1.bandcamp.com/album/jazz-blues",
    ],
}

FMA_TRACKS = {
    "ambient": [
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/swirling-snowflakes-finale/", "Amarent", "Swirling Snowflakes - Finale"),
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/sweet-dreams-middle-eastern-remix/", "Amarent", "Sweet Dreams (Middle-Eastern Remix)"),
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/salt-lake-swerve-chillout-remix/", "Amarent", "Salt Lake Swerve (Chillout Remix)"),
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/cathay-lounge/", "Amarent", "Cathay Lounge"),
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/a-better-world/", "Amarent", "A Better World"),
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/sweet-dreams-2/", "Amarent", "Sweet Dreams"),
        ("https://freemusicarchive.org/music/amarent/free-ambient-music/sweet-love-chill-remix/", "Amarent", "Sweet Love (Chill Remix)"),
        ("https://freemusicarchive.org/music/amarent/free-atmospheric-music/outer-space/", "Amarent", "Outer Space"),
        ("https://freemusicarchive.org/music/amarent/free-atmospheric-music/tuesday-night/", "Amarent", "Tuesday Night"),
        ("https://freemusicarchive.org/music/amarent/free-atmospheric-music/tuesday-night-radio-edit/", "Amarent", "Tuesday Night (Radio Edit)"),
        ("https://freemusicarchive.org/music/amarent/free-atmospheric-music/ethereal-2/", "Amarent", "Ethereal"),
        ("https://freemusicarchive.org/music/Ketsa/modern-meditations/meditation-5/", "Ketsa", "Meditation"),
        ("https://freemusicarchive.org/music/Ketsa/modern-meditations/morning-stillness/", "Ketsa", "Morning Stillness"),
        ("https://freemusicarchive.org/music/Ketsa/modern-meditations/patterns-1/", "Ketsa", "Patterns"),
        ("https://freemusicarchive.org/music/the-imperfectionist/white-noise/1-white-noise-part1mp3/", "The Imperfectionist", "1-White noise part.1.mp3"),
        ("https://freemusicarchive.org/music/the-imperfectionist/white-noise/2-white-noise-part2mp3/", "The Imperfectionist", "2-White noise part.2.mp3"),
        ("https://freemusicarchive.org/music/the-imperfectionist/white-noise/3-white-noise-part3mp3/", "The Imperfectionist", "3-White noise part.3.mp3"),
        ("https://freemusicarchive.org/music/the-imperfectionist/white-noise/4-white-noise-part4mp3/", "The Imperfectionist", "4-White noise part.4.mp3"),
        ("https://freemusicarchive.org/music/the-imperfectionist/white-noise/5-white-noise-part5mp3/", "The Imperfectionist", "5-White noise part.5.mp3"),
        ("https://freemusicarchive.org/music/the-imperfectionist/white-noise/6-white-noise-part6mp3/", "The Imperfectionist", "6-White noise part.6.mp3"),
    ],
}

FMA_EXTRA_TRACKS = {
    "lofi": [
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/tetra/", "Ketsa", "Tetra"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/i-dream-of-you/", "Ketsa", "I Dream Of You"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/black-screen/", "Ketsa", "Black Screen"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/slow-dance/", "Ketsa", "Slow Dance"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/seconds-left/", "Ketsa", "Seconds Left"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/lowest-sun/", "Ketsa", "Lowest Sun"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/down-pitch/", "Ketsa", "Down Pitch"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/reclaimed/", "Ketsa", "Reclaimed"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/the-time-it-takes/", "Ketsa", "The Time It Takes"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/deep-waves/", "Ketsa", "Deep Waves"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/shining-still/", "Ketsa", "Shining Still"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/the-winter-months/", "Ketsa", "The Winter Months"),
        ("https://freemusicarchive.org/music/Ketsa/lofi-downtempo/folded/", "Ketsa", "Folded"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/home-sigh/", "Ketsa", "Home Sigh"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/take-me-up/", "Ketsa", "Take Me Up"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/appointments/", "Ketsa", "Appointments"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/jazz-daze/", "Ketsa", "Jazz Daze"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/bring-dat/", "Ketsa", "Bring Dat"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/make-me-sad/", "Ketsa", "Make Me Sad"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/in-trouble/", "Ketsa", "In Trouble"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/worlds-a-stage/", "Ketsa", "World's A Stage"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/smoothness/", "Ketsa", "Smoothness"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/journal/", "Ketsa", "Journal"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/my-biz/", "Ketsa", "My Biz"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/aligning-frequencies/", "Ketsa", "Aligning Frequencies"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/therapy-1/", "Ketsa", "Therapy"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/sun-slides/", "Ketsa", "Sun Slides"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/to-do/", "Ketsa", "To do"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/grand-rising/", "Ketsa", "Grand Rising"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/the-cure/", "Ketsa", "The Cure"),
        ("https://freemusicarchive.org/music/Ketsa/vintage-beats/keep-hold/", "Ketsa", "Keep Hold"),
        ("https://freemusicarchive.org/music/beat-mekanik/single/one-more/", "JMHBM", "One More"),
        ("https://freemusicarchive.org/music/beat-mekanik/single/night-city/", "JMHBM", "Night City"),
        ("https://freemusicarchive.org/music/beat-mekanik/single/new-new/", "JMHBM", "New New"),
        ("https://freemusicarchive.org/music/beat-mekanik/single/do-me-right/", "JMHBM", "Do Me Right"),
        ("https://freemusicarchive.org/index.php/music/beat-mekanik/single/heavyweights/", "JMHBM", "Heavyweights"),
        ("https://freemusicarchive.org/music/beat-mekanik/single/footsteps/", "JMHBM", "Footsteps"),
        ("https://freemusicarchive.org/music/legacyalli/instrumental-by-legacyalli-2024/rf-lofi-funky-and-chunky/", "legacyAlli", "RF - LoFi Funky and Chunky"),
    ],
}

IA_CURATED_TRACKS = {
    "classic": [
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-01-GoldbergVariationsBwv.988-Aria.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Aria"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-06-GoldbergVariationsBwv.988-Variation5.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 5"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-14-GoldbergVariationsBwv.988-Variation13.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 13"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-32-GoldbergVariationsBwv.988-AriaDaCapo.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Aria Da Capo"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-01-AllegroConBrio.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - I. Allegro con brio"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-02-AdagioMaNonTroppo.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - II. Adagio ma non troppo"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-03-ScherzoAllegro.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - III. Scherzo Allegro"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-04-adagioLaMalinconia.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - IV. La Malinconia"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-01-AllegroModerato.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-02-ScherzoAllegro.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - II. Scherzo Allegro"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-03-NocturneAndante.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - III. Nocturne Andante"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-04-FinaleAndante-Vivace.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - IV. Finale Andante - Vivace"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-01-AllegroMaNonTroppo.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - I. Allegro ma non troppo"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-02-Lento.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - II. Lento"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-03-MoltoVivace.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - III. Molto Vivace"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-04-Finale-VivaceMaNonTroppo.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - IV. Finale Vivace ma non troppo"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-01-AllegroModerato.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-02-AdagioCantabile.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - II. Adagio cantabile"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-03-MenuettoAllegretto.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - III. Menuetto Allegretto"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-04-FinaleVivace.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - IV. Finale Vivace"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-01-AdagioAllegro.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 19 in C Major, K. 465 'Dissonance' - I. Adagio Allegro"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-02-AndanteCantabile.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 19 in C Major, K. 465 'Dissonance' - II. Andante cantabile"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-03-MinuettoAllegretto.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 19 in C Major, K. 465 'Dissonance' - III. Minuetto Allegretto"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-04-AllegroVolto.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 19 in C Major, K. 465 'Dissonance' - IV. Allegro molto"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMajorD.664/FranzSchubert-SonataInAMajorD.664-01-AllegroModerato.mp3", "Franz Schubert", "Sonata in A Major, D. 664 - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMajorD.664/FranzSchubert-SonataInAMajorD.664-02-Andante.mp3", "Franz Schubert", "Sonata in A Major, D. 664 - II. Andante"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMajorD.664/FranzSchubert-SonataInAMajorD.664-03-Allegro.mp3", "Franz Schubert", "Sonata in A Major, D. 664 - III. Allegro"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-01-AllegroModerato.mp3", "Franz Schubert", "Sonata in E-flat Major, D. 568 - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-02-AndanteMolto.mp3", "Franz Schubert", "Sonata in E-flat Major, D. 568 - II. Andante molto"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-03-MenuettoAllegretto.mp3", "Franz Schubert", "Sonata in E-flat Major, D. 568 - III. Menuetto Allegretto"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-04-AllegroModerato.mp3", "Franz Schubert", "Sonata in E-flat Major, D. 568 - IV. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.2inDMajor/JohannesBrahms-SymphonyNo.2InDMajorOp.73-01-AllegroNonTroppo.mp3", "Johannes Brahms", "Symphony No. 2 in D Major, Op. 73 - I. Allegro non troppo"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.2inDMajor/JohannesBrahms-SymphonyNo.2InDMajorOp.73-02-AdagioNonToppo.mp3", "Johannes Brahms", "Symphony No. 2 in D Major, Op. 73 - II. Adagio non troppo"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.2inDMajor/JohannesBrahms-SymphonyNo.2InDMajorOp.73-03-AllegrettoGraziosotake1.mp3", "Johannes Brahms", "Symphony No. 2 in D Major, Op. 73 - III. Allegretto grazioso"),
        ("MusopenCollectionAsFlac", "Suk_Meditation/JosefSuk-Meditation.mp3", "Josef Suk", "Meditation"),
        ("MusopenCollectionAsFlac", "Borodin_InTheSteppesOfCentralAsia/AlexanderBorodin-InTheSteppesOfCentralAsia.mp3", "Alexander Borodin", "In the Steppes of Central Asia"),
        ("MusopenCollectionAsFlac", "Mendelssohn_Hebrides/FelixMendelssohn-HebridesOvertureFingalsCave.mp3", "Felix Mendelssohn", "Hebrides Overture 'Fingal's Cave'"),
        ("MusopenCollectionAsFlac", "Smetana_Vltava/BedichSmetana-MVlast-Vltava.mp3", "Bedrich Smetana", "Ma Vlast - Vltava"),
        ("MusopenCollectionAsFlac", "Mozart_MagicFluteOverture/WolfgangAmadeusMozart-MagicFluteOverture.mp3", "Wolfgang Amadeus Mozart", "Magic Flute Overture"),
        ("MusopenCollectionAsFlac", "Beethoven_EgmontOvertureOp.84/LudwigVanBeethoven-EgmontOvertureOp.84.mp3", "Ludwig van Beethoven", "Egmont Overture, Op. 84"),
    ],
}

# Internet Archive items: (identifier, genre, max_tracks)
IA_ITEMS = [
    ("MusopenCollectionAsFlac", "classic", 40),
    ("Jazz_Sampler-9619", "jazz", 20),
]


def slugify(text: str) -> str:
    """Convert text to a safe filename slug."""
    text = text.lower().strip()
    text = re.sub(r"[^\w\s-]", "", text)
    text = re.sub(r"[\s_]+", "-", text)
    text = re.sub(r"-+", "-", text)
    return text.strip("-")[:80]


def manifest_output_path(genre: str, artist: str, title: str) -> Path:
    slug = slugify(f"{artist}---{title}")
    return MUSIC_DIR / genre / f"{slug}.mp3"


def curated_manifest_tracks(genre: str):
    if genre in FMA_TRACKS:
        return [(artist, title) for _, artist, title in FMA_TRACKS[genre]]
    if genre in IA_CURATED_TRACKS:
        return [(artist, title) for _, _, artist, title in IA_CURATED_TRACKS[genre]]
    return []


def known_track_meta(genre: str):
    meta = {}
    for page_url, artist, title in FMA_TRACKS.get(genre, []):
        meta[manifest_output_path(genre, artist, title)] = (artist, title)
    for page_url, artist, title in FMA_EXTRA_TRACKS.get(genre, []):
        meta[manifest_output_path(genre, artist, title)] = (artist, title)
    for identifier, relative_path, artist, title in IA_CURATED_TRACKS.get(genre, []):
        meta[manifest_output_path(genre, artist, title)] = (artist, title)
    return meta


def download_fma_tracks(genre: str, tracks: list[tuple[str, str, str]]):
    """Download curated FMA tracks by extracting the CDN file URL from each page."""
    out_dir = MUSIC_DIR / genre
    out_dir.mkdir(parents=True, exist_ok=True)
    headers = {"User-Agent": "Mozilla/5.0 (X11; Linux x86_64; late.sh radio)"}

    print(f"\n{'='*60}")
    print(f"  Downloading curated FMA tracks for {genre}")
    print(f"{'='*60}")

    for i, (page_url, artist, title) in enumerate(tracks, start=1):
        out_path = manifest_output_path(genre, artist, title)
        if out_path.exists():
            print(f"  [skip] {artist} - {title}")
            continue

        print(f"  [{i}/{len(tracks)}] {artist} - {title}")
        try:
            req = urllib.request.Request(page_url, headers=headers)
            html = urllib.request.urlopen(req).read().decode("utf-8", errors="replace")

            matches = re.findall(r"files\.freemusicarchive\.org[^\s\"]*\.mp3", html)
            if not matches:
                raise RuntimeError("no mp3 fileUrl found in FMA page")

            cdn_url = "https://" + matches[0].replace("\\/", "/")
            req = urllib.request.Request(cdn_url, headers=headers)
            with urllib.request.urlopen(req) as resp, open(out_path, "wb") as f:
                while True:
                    chunk = resp.read(65536)
                    if not chunk:
                        break
                    f.write(chunk)
        except Exception as e:
            print(f"  [error] {e}")
            if out_path.exists():
                out_path.unlink()


def download_curated_ia_tracks(genre: str, tracks: list[tuple[str, str, str, str]]):
    """Download curated Internet Archive tracks by relative path."""
    out_dir = MUSIC_DIR / genre
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"\n{'='*60}")
    print(f"  Downloading curated Internet Archive tracks for {genre}")
    print(f"{'='*60}")

    for i, (identifier, relative_path, artist, title) in enumerate(tracks, start=1):
        out_path = manifest_output_path(genre, artist, title)
        if out_path.exists():
            print(f"  [skip] {artist} - {title}")
            continue

        print(f"  [{i}/{len(tracks)}] {artist} - {title}")
        try:
            dl_url = f"https://archive.org/download/{identifier}/{urllib.request.quote(relative_path)}"
            urllib.request.urlretrieve(dl_url, str(out_path))
        except Exception as e:
            print(f"  [error] {e}")
            if out_path.exists():
                out_path.unlink()


def download_bandcamp(genre: str, urls: list[str]):
    """Download albums from Bandcamp using yt-dlp."""
    out_dir = MUSIC_DIR / genre
    out_dir.mkdir(parents=True, exist_ok=True)

    for url in urls:
        print(f"\n{'='*60}")
        print(f"  Downloading: {url}")
        print(f"  Genre: {genre}")
        print(f"{'='*60}")

        # yt-dlp outputs MP3 with metadata
        cmd = [
            "yt-dlp",
            "--extract-audio",
            "--audio-format", "mp3",
            "--audio-quality", "128K",
            "--output", str(out_dir / "%(artist)s---%(title)s.%(ext)s"),
            "--no-overwrites",
            "--ignore-errors",
            "--no-playlist" if "/track/" in url else "--yes-playlist",
            url,
        ]
        subprocess.run(cmd)


def download_ia(identifier: str, genre: str, max_tracks: int):
    """Download audio files from Internet Archive."""
    out_dir = MUSIC_DIR / genre
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"\n{'='*60}")
    print(f"  Downloading from Internet Archive: {identifier}")
    print(f"  Genre: {genre} (max {max_tracks} tracks)")
    print(f"{'='*60}")

    # Fetch item metadata
    meta_url = f"https://archive.org/metadata/{identifier}"
    req = urllib.request.Request(meta_url, headers={"User-Agent": "late.sh-radio/1.0"})
    resp = urllib.request.urlopen(req)
    meta = json.loads(resp.read())

    server = meta.get("server", "archive.org")
    dir_ = meta.get("dir", "")
    item_meta = meta.get("metadata", {})

    count = 0
    for f in meta.get("files", []):
        if count >= max_tracks:
            break

        fmt = f.get("format", "")
        name = f.get("name", "")

        # Accept MP3 or FLAC files
        is_mp3 = name.endswith(".mp3") and "MP3" in fmt
        is_flac = name.endswith(".flac")
        is_ogg = name.endswith(".ogg")

        if not (is_mp3 or is_flac or is_ogg):
            continue

        # Extract metadata
        title = f.get("title", Path(name).stem)
        creator = f.get("creator", item_meta.get("creator", "Unknown"))
        if isinstance(creator, list):
            creator = creator[0]

        # Clean up title - remove movement indicators for classical if too long
        title = str(title).replace('"', "'")
        creator = str(creator).replace('"', "'")

        slug = slugify(f"{creator}---{title}")
        dl_url = f"https://{server}{dir_}/{urllib.request.quote(name)}"

        if is_mp3:
            out_path = out_dir / f"{slug}.mp3"
        else:
            out_path = out_dir / f"{slug}.mp3"  # will convert

        if out_path.exists():
            print(f"  [skip] {out_path.name}")
            count += 1
            continue

        print(f"  [{count+1}/{max_tracks}] {creator} - {title}")

        try:
            if is_mp3:
                urllib.request.urlretrieve(dl_url, str(out_path))
            else:
                # Download FLAC/OGG then convert to MP3
                tmp_path = out_dir / f"{slug}{Path(name).suffix}"
                urllib.request.urlretrieve(dl_url, str(tmp_path))
                subprocess.run([
                    "ffmpeg", "-i", str(tmp_path),
                    "-ab", "128k", "-ar", "44100",
                    "-y", "-loglevel", "error",
                    str(out_path),
                ], check=True)
                tmp_path.unlink()

            count += 1
        except Exception as e:
            print(f"  [error] {e}")
            if out_path.exists():
                out_path.unlink()

    print(f"  Downloaded {count} tracks for {genre}")


def generate_m3u(genre: str):
    """Generate .m3u playlist from downloaded MP3 files."""
    music_path = MUSIC_DIR / genre
    m3u_path = LIQUIDSOAP_DIR / f"{genre}.m3u"

    manifest_tracks = curated_manifest_tracks(genre)
    manifest_meta = known_track_meta(genre)
    if manifest_tracks:
        mp3_files = []
        for artist, title in manifest_tracks:
            path = manifest_output_path(genre, artist, title)
            if path.exists():
                mp3_files.append(path)
            else:
                print(f"  [warn] Missing curated track: {path.name}")
    else:
        mp3_files = sorted(music_path.glob("*.mp3"))
    if not mp3_files:
        print(f"  [warn] No MP3 files found in {music_path}")
        return

    lines = []
    for mp3 in mp3_files:
        is_curated = mp3 in manifest_meta
        if is_curated:
            artist, title = manifest_meta[mp3]
        else:
            stem = mp3.stem
            # Parse artist---title from filename
            if "---" in stem:
                parts = stem.split("---", 1)
                artist = parts[0].replace("-", " ").title()
                title = parts[1].replace("-", " ").title()
            else:
                artist = "Unknown"
                title = stem.replace("-", " ").title()

        # Try to get metadata + duration from ffprobe
        duration = ""
        try:
            result = subprocess.run(
                ["ffprobe", "-v", "quiet", "-print_format", "json",
                 "-show_format", str(mp3)],
                capture_output=True, text=True, timeout=5,
            )
            if result.returncode == 0:
                probe = json.loads(result.stdout)
                fmt = probe.get("format", {})
                tags = fmt.get("tags", {})
                if not is_curated and tags.get("artist"):
                    artist = tags["artist"].replace('"', "'")
                if not is_curated and tags.get("title"):
                    title = tags["title"].replace('"', "'")
                dur_secs = float(fmt.get("duration", 0))
                if dur_secs > 0:
                    duration = str(int(dur_secs))
        except Exception:
            pass

        if title.endswith(".mp3"):
            title = re.sub(r"^\d+-", "", title)
            title = title[:-4]
            title = title.replace(".", " ").strip().title()

        # Strip artist prefix from title (Bandcamp often encodes "Artist - Title")
        prefixes = [f"{artist} - ", f"{artist} — ", f"{artist}   "]
        for prefix in prefixes:
            if title.startswith(prefix):
                title = title[len(prefix):]
                break

        # Container path (mounted as /music/<genre>/)
        container_path = f"/music/{genre}/{mp3.name}"
        dur_part = f',duration="{duration}"' if duration else ""
        lines.append(f'annotate:artist="{artist}",title="{title}"{dur_part}:{container_path}')

    with open(m3u_path, "w") as f:
        f.write("\n".join(lines) + "\n")

    print(f"  Generated {m3u_path.name}: {len(lines)} tracks")


def main():
    global MUSIC_DIR, LIQUIDSOAP_DIR

    parser = argparse.ArgumentParser(description="Fetch CC music for late.sh radio")
    parser.add_argument("--genre", default="all",
                        choices=["lofi", "ambient", "classic", "jazz", "all"],
                        help="Which genre to download (default: all)")
    parser.add_argument("--music-dir", type=Path, default=DEFAULT_MUSIC_DIR,
                        help="Where to store downloaded music (default: repo music/)")
    parser.add_argument("--liquidsoap-dir", type=Path, default=DEFAULT_LIQUIDSOAP_DIR,
                        help="Where to write generated .m3u files (default: repo infra/liquidsoap/)")
    parser.add_argument("--m3u-only", action="store_true",
                        help="Only regenerate .m3u files from existing downloads")
    parser.add_argument("--skip-m3u", action="store_true",
                        help="Skip generating .m3u files")
    args = parser.parse_args()

    MUSIC_DIR = args.music_dir.resolve()
    LIQUIDSOAP_DIR = args.liquidsoap_dir.resolve()

    genres = ["lofi", "ambient", "classic", "jazz"] if args.genre == "all" else [args.genre]

    if not args.m3u_only:
        for genre in genres:
            if genre in FMA_TRACKS:
                download_fma_tracks(genre, FMA_TRACKS[genre])
            if genre in FMA_EXTRA_TRACKS:
                download_fma_tracks(genre, FMA_EXTRA_TRACKS[genre])
            if genre in IA_CURATED_TRACKS:
                download_curated_ia_tracks(genre, IA_CURATED_TRACKS[genre])

        # Download from Bandcamp
        for genre in genres:
            if genre in BANDCAMP_ALBUMS:
                download_bandcamp(genre, BANDCAMP_ALBUMS[genre])

        # Download from Internet Archive
        for identifier, genre, max_tracks in IA_ITEMS:
            if genre in genres:
                download_ia(identifier, genre, max_tracks)

    if not args.skip_m3u:
        print(f"\n{'='*60}")
        print("  Generating .m3u playlists")
        print(f"{'='*60}")
        for genre in genres:
            generate_m3u(genre)

        print("\nDone! Next steps:")
        print(f"  1. Review the generated .m3u files in {LIQUIDSOAP_DIR}/")
        print("  2. Update radio.liq to remove input.http() streams")
        print("  3. Restart liquidsoap: docker compose restart liquidsoap")
    else:
        print("\nDone! Skipped .m3u generation.")


if __name__ == "__main__":
    main()
