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
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-02-GoldbergVariationsBwv.988-Variation1.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 1"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-03-GoldbergVariationsBwv.988-Variation2.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 2"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-04-GoldbergVariationsBwv.988-Variation3.CanonOnTheUnison.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 3. Canon on the unison"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-05-GoldbergVariationsBwv.988-Variation4.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 4"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-06-GoldbergVariationsBwv.988-Variation5.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 5"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-07-GoldbergVariationsBwv.988-Variation6.CanonOnTheSecond.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 6. Canon on the second"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-08-GoldbergVariationsBwv.988-Variation7.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 7"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-09-GoldbergVariationsBwv.988-Variation8.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 8"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-10-GoldbergVariationsBwv.988-Variation9.CanonOnTheThird.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 9. Canon on the third"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-11-GoldbergVariationsBwv.988-Variation10.Fughetta.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 10. Fughetta"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-12-GoldbergVariationsBwv.988-Variation11.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 11"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-13-GoldbergVariationsBwv.988-Variation12.CanonOnTheFourth.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 12. Canon on the fourth"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-14-GoldbergVariationsBwv.988-Variation13.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 13"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-15-GoldbergVariationsBwv.988-Variation14.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 14"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-16-GoldbergVariationsBwv.988-Variation15.CanonOnTheFifth.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 15. Canon on the fifth"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-17-GoldbergVariationsBwv.988-Variation16.Overture.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 16. Overture"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-18-GoldbergVariationsBwv.988-Variation17.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 17"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-19-GoldbergVariationsBwv.988-Variation18.CanonOnTheSixth.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 18. Canon on the sixth"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-20-GoldbergVariationsBwv.988-Variation19.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 19"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-21-GoldbergVariationsBwv.988-Variation20.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 20"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-22-GoldbergVariationsBwv.988-Variation21.CanonOnTheSeventh.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 21. Canon on the seventh"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-23-GoldbergVariationsBwv.988-Variation22.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 22"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-24-GoldbergVariationsBwv.988-Variation23.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 23"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-25-GoldbergVariationsBwv.988-Variation24.CanonOnTheOctave.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 24. Canon on the octave"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-26-GoldbergVariationsBwv.988-Variation25.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 25"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-27-GoldbergVariationsBwv.988-Variation26.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 26"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-28-GoldbergVariationsBwv.988-Variation27.CanonOnTheNinth.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 27. Canon on the ninth"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-29-GoldbergVariationsBwv.988-Variation28.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 28"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-30-GoldbergVariationsBwv.988-Variation29.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 29"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-31-GoldbergVariationsBwv.988-Variation30.Quodlibet.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Variation 30. Quodlibet"),
        ("MusopenCollectionAsFlac", "Bach_GoldbergVariations/JohannSebastianBach-32-GoldbergVariationsBwv.988-AriaDaCapo.mp3", "Johann Sebastian Bach", "Goldberg Variations, BWV 988 - Aria Da Capo"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-01-AllegroConBrio.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - I. Allegro con brio"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-02-AdagioMaNonTroppo.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - II. Adagio ma non troppo"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-03-ScherzoAllegro.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - III. Scherzo Allegro"),
        ("MusopenCollectionAsFlac", "Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-04-adagioLaMalinconia.mp3", "Ludwig van Beethoven", "String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - IV. La Malinconia"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-01-AllegroModerato.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 15 in D Minor, K. 421 - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Beethoven_SymphonyNo.3Eroica/LudwigVanBeethoven-SymphonyNo.3InEFlatMajorEroicaOp.55-02-MarciaFunebreAdagioAssai.mp3", "Ludwig van Beethoven", "Symphony No. 3 in E Flat Major Eroica, Op. 55 - 02 - Marcia funebre Adagio assai"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-02-Andante.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 15 in D Minor, K. 421 - II. Andante"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-03-Minuetto.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 15 in D Minor, K. 421 - III. Minuetto"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.1inAMajor/AlexanderBorodin-StringQuartetNo.1InAMajor-01-Moderato-Allegro.mp3", "Alexander Borodin", "String Quartet No. 1 in A Major - 01 - Moderato - Allegro"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.1inAMajor/AlexanderBorodin-StringQuartetNo.1InAMajor-02-AndanteConMoto.mp3", "Alexander Borodin", "String Quartet No. 1 in A Major - 02 - Andante con moto"),
        ("MusopenCollectionAsFlac", "Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-04-AllegroMaNonTroppo.mp3", "Wolfgang Amadeus Mozart", "String Quartet No. 15 in D Minor, K. 421 - IV. Allegro ma non troppo"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.1inAMajor/AlexanderBorodin-StringQuartetNo.1InAMajor-04-Andante-AllegroRisoluto.mp3", "Alexander Borodin", "String Quartet No. 1 in A Major - 04 - Andante - Allegro risoluto"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-01-AllegroModerato.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-02-ScherzoAllegro.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - II. Scherzo Allegro"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-03-NocturneAndante.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - III. Nocturne Andante"),
        ("MusopenCollectionAsFlac", "Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-04-FinaleAndante-Vivace.mp3", "Alexander Borodin", "String Quartet No. 2 in D Major - IV. Finale Andante - Vivace"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMinorD.845/FranzSchubert-SonataInAMinorD.845-01-Moderato.mp3", "Franz Schubert", "Sonata in A Minor, D. 845 - I. Moderato"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.1inCMinor/JohannesBrahms-SymphonyNo.1InCMinorOp.68-02-AndanteSostenuto.mp3", "Johannes Brahms", "Symphony No. 1 in C Minor, Op. 68 - 02 - Andante sostenuto"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.1inCMinor/JohannesBrahms-SymphonyNo.1InCMinorOp.68-03-UnPocoAllegrettoEGrazioso.mp3", "Johannes Brahms", "Symphony No. 1 in C Minor, Op. 68 - 03 - Un poco allegretto e grazioso"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMinorD.845/FranzSchubert-SonataInAMinorD.845-02-AndantePocoMosso.mp3", "Franz Schubert", "Sonata in A Minor, D. 845 - II. Andante poco mosso"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-01-AllegroConBrio.mp3", "Johannes Brahms", "Symphony No. 3 in F Major, Op. 90 - 01 - Allegro con brio"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-02-Andante.mp3", "Johannes Brahms", "Symphony No. 3 in F Major, Op. 90 - 02 - Andante"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-03-PocoAllegretto.mp3", "Johannes Brahms", "Symphony No. 3 in F Major, Op. 90 - 03 - Poco allegretto"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-04-Allegro.mp3", "Johannes Brahms", "Symphony No. 3 in F Major, Op. 90 - 04 - Allegro"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.4inEMinor/JohannesBrahms-SymphonyNo.4InEMinorOp.98-01-AllegroNonTroppo.mp3", "Johannes Brahms", "Symphony No. 4 in E Minor, Op. 98 - 01 - Allegro Non Troppo"),
        ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.4inEMinor/JohannesBrahms-SymphonyNo.4InEMinorOp.98-02-AndanteModerato.mp3", "Johannes Brahms", "Symphony No. 4 in E Minor, Op. 98 - 02 - Andante Moderato"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMinorD.959/FranzSchubert-SonataInAMinorD.959-02-Andantino.mp3", "Franz Schubert", "Sonata in A Minor, D. 959 - II. Andantino"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMinorD.959/FranzSchubert-SonataInAMinorD.959-04-Rondo.Allegretto.mp3", "Franz Schubert", "Sonata in A Minor, D. 959 - IV. Rondo Allegretto"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-01-AllegroMaNonTroppo.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - I. Allegro ma non troppo"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-02-Lento.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - II. Lento"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInCMinorD.958/FranzSchubert-SonataInCMinorD.958-02-Adagio.mp3", "Franz Schubert", "Sonata in C Minor, D. 958 - II. Adagio"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-04-Finale-VivaceMaNonTroppo.mp3", "Antonin Dvorak", "String Quartet No. 12 in F Major, Op. 96 'American' - IV. Finale Vivace ma non troppo"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.10inEFlatOp.51/AntonnDvorak-StringQuartetNo.10InEFlatOp.51-01-AllegroMaNonTroppo.mp3", "Antonin Dvorak", "String Quartet No. 10 in E Flat, Op. 51 - 01 - Allegro Ma Non Troppo"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.10inEFlatOp.51/AntonnDvorak-StringQuartetNo.10InEFlatOp.51-02-Dumka.mp3", "Antonin Dvorak", "String Quartet No. 10 in E Flat, Op. 51 - 02 - Dumka"),
        ("MusopenCollectionAsFlac", "Dvorak_StringQuartetNo.10inEFlatOp.51/AntonnDvorak-StringQuartetNo.10InEFlatOp.51-03-Romanza.mp3", "Antonin Dvorak", "String Quartet No. 10 in E Flat, Op. 51 - 03 - Romanza"),
        ("MusopenCollectionAsFlac", "Schubert_SonataInAMinorD.784/FranzSchubert-SonataInAMinorD.784-02-Andante.mp3", "Franz Schubert", "Sonata in A Minor, D. 784 - II. Andante"),
        ("MusopenCollectionAsFlac", "Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-01-Morning.mp3", "Edvard Grieg", "Peer Gynt Suite No. 1, Op. 46 - 01 - Morning"),
        ("MusopenCollectionAsFlac", "Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-02-AasesDeath.mp3", "Edvard Grieg", "Peer Gynt Suite No. 1, Op. 46 - 02 - Aase's Death"),
        ("MusopenCollectionAsFlac", "Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-03-AnitrasDream.mp3", "Edvard Grieg", "Peer Gynt Suite No. 1, Op. 46 - 03 - Anitra's Dream"),
        ("MusopenCollectionAsFlac", "Mendelssohn_ScottishSymphony/FelixMendelssohn-SymphonyNo.3InAMinorscottishOp.56-01-AndanteConMoto.mp3", "Felix Mendelssohn", "Symphony No. 3 in A Minor 'Scottish', Op. 56 - I. Andante con moto"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-01-AllegroModerato.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - I. Allegro moderato"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-02-AdagioCantabile.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - II. Adagio cantabile"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-03-MenuettoAllegretto.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - III. Menuetto Allegretto"),
        ("MusopenCollectionAsFlac", "Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-04-FinaleVivace.mp3", "Joseph Haydn", "String Quartet in D Major, Op. 64 No. 5 'Lark' - IV. Finale Vivace"),
        ("MusopenCollectionAsFlac", "Mendelssohn_ItalianSymphony/FelixMendelssohn-SymphonyNo.4InAMajorOp.90italian-01-AllegroVivace.mp3", "Felix Mendelssohn", "Symphony No. 4 in A Major, Op. 90 'Italian' - 01 - Allegro vivace"),
        ("MusopenCollectionAsFlac", "Mendelssohn_ItalianSymphony/FelixMendelssohn-SymphonyNo.4InAMajorOp.90italian-02-AndanteConMoto.mp3", "Felix Mendelssohn", "Symphony No. 4 in A Major, Op. 90 'Italian' - 02 - Andante con moto"),
        ("MusopenCollectionAsFlac", "Mendelssohn_ItalianSymphony/FelixMendelssohn-SymphonyNo.4InAMajorOp.90italian-03-ConMotoModerato.mp3", "Felix Mendelssohn", "Symphony No. 4 in A Major, Op. 90 'Italian' - 03 - Con moto moderato"),
        ("MusopenCollectionAsFlac", "Mendelssohn_ScottishSymphony/FelixMendelssohn-SymphonyNo.3InAMinorscottishOp.56-03-Adagio.mp3", "Felix Mendelssohn", "Symphony No. 3 in A Minor 'Scottish', Op. 56 - III. Adagio"),
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
        ("MusopenCollectionAsFlac", "Mozart_SymphonyNo.40inGMinor/WolfgangAmadeusMozart-SymphonyNo.40InGMinorK.550-02-Andante.mp3", "Wolfgang Amadeus Mozart", "Symphony No. 40 in G Minor, K. 550 - II. Andante"),
        ("MusopenCollectionAsFlac", "Mozart_SymphonyNo.40inGMinor/WolfgangAmadeusMozart-SymphonyNo.40InGMinorK.550-03-MenuettoAllegretto.mp3", "Wolfgang Amadeus Mozart", "Symphony No. 40 in G Minor, K. 550 - III. Menuetto Allegretto"),
    ],
}

# Internet Archive items: (identifier, genre, max_tracks)
IA_ITEMS = [
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
            "--trim-filenames", "120",
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
