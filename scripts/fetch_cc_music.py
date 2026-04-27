#!/usr/bin/env python3
"""
Download CC0/CC-BY music for late.sh radio and generate .m3u playlists.

Sources:
  Lofi:      HoliznaCC0 (CC0) via Bandcamp
  Ambient:   Curated FMA ambient set (CC-BY 4.0)
  Classical: Musopen (Public Domain) via Internet Archive
  Jazz:      Kevin MacLeod (CC-BY) via Internet Archive + HoliznaCC0 (CC0) via Bandcamp

All downloads land in tmp/<genre>/ (the upload-staging area). Dev fixtures
under music/<genre>/ and R2-backed production tracks are never touched. After
the user confirms upload to R2, staged files in tmp/ should be removed.

Dependencies: yt-dlp, ffmpeg, python3
Usage: python3 scripts/fetch_cc_music.py [--genre lofi|ambient|classic|jazz|all] [--music-dir PATH] [--skip-m3u]
"""

import subprocess, json, os, sys, re, urllib.request, glob, argparse
from pathlib import Path

DEFAULT_MUSIC_DIR = Path(__file__).resolve().parent.parent / "tmp"
DEFAULT_LIQUIDSOAP_DIR = Path(__file__).resolve().parent.parent / "infra" / "liquidsoap"
MUSIC_DIR = DEFAULT_MUSIC_DIR
LIQUIDSOAP_DIR = DEFAULT_LIQUIDSOAP_DIR

# ---------------------------------------------------------------------------
# Source definitions
# ---------------------------------------------------------------------------

BANDCAMP_ALBUMS = {
    "lofi": [
        "https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2",
        "https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer",
    ],
    "jazz": [
        "https://holiznacc0.bandcamp.com/album/lofi-jazz-guitar",
        "https://kevinmacleod1.bandcamp.com/album/jazz-blues",
    ],
}

FMA_TRACKS = {
    "ambient": [
        ("https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_01_Closer_To_You/", "Sergey Cheremisinov", "Closer To You"),
        ("https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_02_Train/", "Sergey Cheremisinov", "Train"),
        ("https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_03_Waves/", "Sergey Cheremisinov", "Waves"),
        ("https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_04_When_You_Leave/", "Sergey Cheremisinov", "When You Leave"),
        ("https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_05_Fog/", "Sergey Cheremisinov", "Fog"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_01_Fouler_lhorizon/", "Komiku", "Fouler l'horizon"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_02_Le_Grand_Village/", "Komiku", "Le Grand Village"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_03_Champ_de_tournesol/", "Komiku", "Champ de tournesol"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_04_Barque_sur_le_lac/", "Komiku", "Barque sur le lac"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_09_De_lherbe_sous_les_pieds/", "Komiku", "De l'herbe sous les pieds"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_13_Bleu/", "Komiku", "Bleu"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_14_Un_coin_loin_du_monde/", "Komiku", "Un coin loin du monde"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_01_Balance/", "Komiku", "Balance"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_02_Chill_Out_Theme/", "Komiku", "Chill Out Theme"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_04_Time/", "Komiku", "Time"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_05_Down_the_river/", "Komiku", "Down the river"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_07_Frozen_Jungle/", "Komiku", "Frozen Jungle"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_08_Dreaming_of_you/", "Komiku", "Dreaming of you"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_3/Komiku_-_Its_time_for_adventure_vol_3_-_01_Childhood_scene/", "Komiku", "Childhood scene"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_3/Komiku_-_Its_time_for_adventure_vol_3_-_07_The_place_that_never_get_old/", "Komiku", "The place that never gets old"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_5/Komiku_-_Its_time_for_adventure_vol_5_-_05_Xenobiological_Forest/", "Komiku", "Xenobiological Forest"),
        ("https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_5/Komiku_-_Its_time_for_adventure_vol_5_-_06_Friendss_theme/", "Komiku", "Friends's theme"),
        ("https://freemusicarchive.org/music/holiznacc0/lullabies-for-the-end-of-the-world/lullabies-for-the-end-of-the-world-1/", "HoliznaCC0", "Lullabies For The End Of The World 1"),
        ("https://freemusicarchive.org/music/holiznacc0/lullabies-for-the-end-of-the-world/lullabies-for-the-end-of-the-world-2/", "HoliznaCC0", "Lullabies For The End Of The World 2"),
        ("https://freemusicarchive.org/music/holiznacc0/lullabies-for-the-end-of-the-world/lullabies-for-the-end-of-the-world-3/", "HoliznaCC0", "Lullabies For The End Of The World 3"),
    ],
}

FMA_EXTRA_TRACKS = {
    "lofi": [
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-1/", "HoliznaCC0", "OST Music Box 1"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-2/", "HoliznaCC0", "OST Music Box 2"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-3/", "HoliznaCC0", "OST Music Box 3"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-4/", "HoliznaCC0", "OST Music Box 4"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-5/", "HoliznaCC0", "OST Music Box 5"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-6/", "HoliznaCC0", "OST Music Box 6"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-7/", "HoliznaCC0", "OST Music Box 7"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/drifting-piano/", "HoliznaCC0", "Drifting Piano"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/a-small-town-on-pluto-music-box/", "HoliznaCC0", "A Small Town On Pluto (Music Box)"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/a-small-town-on-pluto-composed/", "HoliznaCC0", "A Small Town On Pluto (Composed)"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/game-travel-1-piano/", "HoliznaCC0", "Game Travel 1 (Piano)"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/vst-guitar/", "HoliznaCC0", "VST Guitar"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/cabin-fever/", "HoliznaCC0", "Cabin Fever"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/spring-on-the-horizon/", "HoliznaCC0", "Spring On The Horizon"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-1/", "HoliznaCC0", "Creepy Piano 1"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-2/", "HoliznaCC0", "Creepy Piano 2"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-3/", "HoliznaCC0", "Creepy Piano 3"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-4/", "HoliznaCC0", "Creepy Piano 4"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/dangerous-voyage/", "HoliznaCC0", "Dangerous Voyage"),
        ("https://freemusicarchive.org/music/holiznacc0/background-music/dangerous-voyage-music-box/", "HoliznaCC0", "Dangerous Voyage (Music Box)"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/saviour-above/", "Ketsa", "Saviour Above"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/always-faithful/", "Ketsa", "Always Faithful"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/all-ways/", "Ketsa", "All Ways"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/feeling-1/", "Ketsa", "Feeling"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/importance-1/", "Ketsa", "Importance"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/trench-work/", "Ketsa", "Trench Work"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/night-flow-day-grow/", "Ketsa", "Night Flow Day Grow"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/will-make-you-happy/", "Ketsa", "Will Make You Happy"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/bright-state/", "Ketsa", "Bright State"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/cello/", "Ketsa", "Cello"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/dry-and-high/", "Ketsa", "Dry and High"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/the-road-1/", "Ketsa", "The Road"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/kinship/", "Ketsa", "Kinship"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/her-memory-fading/", "Ketsa", "Her Memory Fading"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/what-it-feels-like-1/", "Ketsa", "What It Feels Like"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/a-little-faith/", "Ketsa", "A Little Faith"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/tide-turns/", "Ketsa", "Tide Turns"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/longer-wait/", "Ketsa", "Longer Wait"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/life-is-great/", "Ketsa", "Life is Great"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/that-feeling/", "Ketsa", "That Feeling"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/london-west/", "Ketsa", "London West"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/dawn-faded/", "Ketsa", "Dawn Faded"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/good-feel/", "Ketsa", "Good Feel"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/here-for-you/", "Ketsa", "Here For You"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/brazilian-sunsets/", "Ketsa", "Brazilian Sunsets"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/too-late/", "Ketsa", "Too Late"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/the-road-2/", "Ketsa", "The Road 2"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/off-days/", "Ketsa", "Off Days"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/inside-dead/", "Ketsa", "Inside Dead"),
        ("https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/vision-2/", "Ketsa", "Vision"),
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

# Second-pass classical expansion: ~100 chill picks for coding sessions, drawn
# from explicitly CC0 / Public-Domain-marked archive.org items. These are
# downloaded into tmp/classic/ and APPENDED to infra/liquidsoap/classic.m3u
# (the existing 100-track manifest in R2 is left untouched). Triggered by
# --classic-expand on the CLI.
#
# Sources:
#   - musopen-chopin (CC0): Musopen's Complete Chopin Collection
#   - bach-well-tempered-clavier-book-1 (PD mark): Kimiko Ishizaka's "Open
#     Well-Tempered Clavier" Book I, the canonical PD recording
#   - MusopenCollectionAsFlac (PD): chill movements not yet in the manifest
IA_CLASSIC_EXPANSION = [
    # ---- MusopenCollectionAsFlac: chill movements left over from pass 1 ----
    ("MusopenCollectionAsFlac", "Mendelssohn_StringQuartetNo.6inFMinorOp.80/FelixMendelssohn-StringQuartetNo.6InFMinorOp.80-03-Adagio.mp3", "Felix Mendelssohn", "String Quartet No. 6 in F Minor, Op. 80 - III. Adagio"),
    ("MusopenCollectionAsFlac", "Schubert_SonataInDMajorD.850/FranzSchubert-SonataInDMajorD.850-02-ConMoto.mp3", "Franz Schubert", "Sonata in D Major, D. 850 - II. Con moto"),
    ("MusopenCollectionAsFlac", "Tchaikovsky_SymphonyPathetique/PyotrIlyichTchaikovsky-SymphonyNo.6InBMinorOp.74pathtique-04-FinaleAdagioLamentoso.mp3", "Pyotr Ilyich Tchaikovsky", "Symphony No. 6 in B Minor, Op. 74 'Pathetique' - IV. Finale Adagio lamentoso"),
    ("MusopenCollectionAsFlac", "Brahms_SymphonyNo.1inCMinor/JohannesBrahms-SymphonyNo.1InCMinorOp.68-04-Adagio-PiAndante-AllegroNonTroppoMaConBrio.mp3", "Johannes Brahms", "Symphony No. 1 in C Minor, Op. 68 - IV. Adagio - Piu andante - Allegro non troppo"),
    ("MusopenCollectionAsFlac", "Schubert_SonataInCMinorD.958/FranzSchubert-SonataInCMinorD.958-03-MenuettoAllegro.mp3", "Franz Schubert", "Sonata in C Minor, D. 958 - III. Menuetto Allegro"),

    # ---- Kimiko Ishizaka, Open Well-Tempered Clavier Book I (CC0/PD mark) ----
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 01 Prelude No. 1 in C major, BWV 846.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 1 in C major, BWV 846"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 02 Fugue No. 1 in C major, BWV 846.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 1 in C major, BWV 846"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 03 Prelude No. 2 in C minor, BWV 847.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 2 in C minor, BWV 847"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 04 Fugue No. 2 in C minor, BWV 847.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 2 in C minor, BWV 847"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 05 Prelude No. 3 in C-sharp major, BWV 848.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 3 in C-sharp major, BWV 848"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 06 Fugue No. 3 in C-sharp major, BWV 848.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 3 in C-sharp major, BWV 848"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 07 Prelude No. 4 in C-sharp minor, BWV 849.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 4 in C-sharp minor, BWV 849"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 08 Fugue No. 4 in C-sharp minor, BWV 849.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 4 in C-sharp minor, BWV 849"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 09 Prelude No. 5 in D major, BWV 850.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 5 in D major, BWV 850"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 10 Fugue No. 5 in D major, BWV 850.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 5 in D major, BWV 850"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 11 Prelude No. 6 in D minor, BWV 851.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 6 in D minor, BWV 851"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 12 Fugue No. 6 in D minor, BWV 851.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 6 in D minor, BWV 851"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 13 Prelude No. 7 in E-flat major, BWV 852.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 7 in E-flat major, BWV 852"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 14 Fugue No. 7 in E-flat major, BWV 852.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 7 in E-flat major, BWV 852"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 15 Prelude No. 8 in E-flat minor, BWV 853.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 8 in E-flat minor, BWV 853"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 16 Fugue No. 8 in D-sharp minor, BWV 853.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 8 in D-sharp minor, BWV 853"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 17 Prelude No. 9 in E major, BWV 854.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 9 in E major, BWV 854"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 18 Fugue No. 9 in E major, BWV 854.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 9 in E major, BWV 854"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 19 Prelude No. 10 in E minor, BWV 855.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 10 in E minor, BWV 855"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 20 Fugue No. 10 in E minor, BWV 855.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 10 in E minor, BWV 855"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 21 Prelude No. 11 in F major, BWV 856.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 11 in F major, BWV 856"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 22 Fugue No. 11 in F major, BWV 856.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 11 in F major, BWV 856"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 23 Prelude No. 12 in F minor, BWV 857.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 12 in F minor, BWV 857"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 24 Fugue No. 12 in F minor, BWV 857.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 12 in F minor, BWV 857"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 25 Prelude No. 13 in F-sharp major, BWV 858.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 13 in F-sharp major, BWV 858"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 26 Fugue No. 13 in F-sharp major, BWV 858.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 13 in F-sharp major, BWV 858"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 27 Prelude No. 14 in F-sharp minor, BWV 859.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 14 in F-sharp minor, BWV 859"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 28 Fugue No. 14 in F-sharp minor, BWV 859.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 14 in F-sharp minor, BWV 859"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 29 Prelude No. 15 in G major, BWV 860.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 15 in G major, BWV 860"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 30 Fugue No. 15 in G major, BWV 860.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 15 in G major, BWV 860"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 31 Prelude No. 16 in G minor, BWV 861.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 16 in G minor, BWV 861"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 32 Fugue No. 16 in G minor, BWV 861.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 16 in G minor, BWV 861"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 33 Prelude No. 17 in A-flat major, BWV 862.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 17 in A-flat major, BWV 862"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 34 Fugue No. 17 in A-flat major, BWV 862.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 17 in A-flat major, BWV 862"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 35 Prelude No. 18 in G-sharp minor, BWV 863.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 18 in G-sharp minor, BWV 863"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 36 Fugue No. 18 in G-sharp minor, BWV 863.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 18 in G-sharp minor, BWV 863"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 37 Prelude No. 19 in A major, BWV 864.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 19 in A major, BWV 864"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 38 Fugue No. 19 in A major, BWV 864.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 19 in A major, BWV 864"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 39 Prelude No. 20 in A minor, BWV 865.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 20 in A minor, BWV 865"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 40 Fugue No. 20 in A minor, BWV 865.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 20 in A minor, BWV 865"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 41 Prelude No. 21 in B-flat major, BWV 866.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 21 in B-flat major, BWV 866"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 42 Fugue No. 21 in B-flat major, BWV 866.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 21 in B-flat major, BWV 866"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 43 Prelude No. 22 in B-flat minor, BWV 867.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 22 in B-flat minor, BWV 867"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 44 Fugue No. 22 in B-flat minor, BWV 867.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 22 in B-flat minor, BWV 867"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 45 Prelude No. 23 in B major, BWV 868.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 23 in B major, BWV 868"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 46 Fugue No. 23 in B major, BWV 868.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 23 in B major, BWV 868"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 47 Prelude No. 24 in B minor, BWV 869.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Prelude No. 24 in B minor, BWV 869"),
    ("bach-well-tempered-clavier-book-1", "Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 48 Fugue No. 24 in B minor, BWV 869.mp3", "J.S. Bach (Kimiko Ishizaka)", "Well-Tempered Clavier Book I - Fugue No. 24 in B minor, BWV 869"),

    # ---- Musopen Complete Chopin Collection (CC0) - Nocturnes ----
    ("musopen-chopin", "NocturneOp.9No.1InBFlatMinor.mp3", "Frederic Chopin", "Nocturne Op. 9 No. 1 in B-flat minor"),
    ("musopen-chopin", "Nocturne Op. 9 no. 2 in E flat major.mp3", "Frederic Chopin", "Nocturne Op. 9 No. 2 in E-flat major"),
    ("musopen-chopin", "NocturneOp.9No.3.mp3", "Frederic Chopin", "Nocturne Op. 9 No. 3 in B major"),
    ("musopen-chopin", "Nocturne Op. 15 no. 1 In F major.mp3", "Frederic Chopin", "Nocturne Op. 15 No. 1 in F major"),
    ("musopen-chopin", "Nocturne Op. 27 no. 1 in C sharp minor.mp3", "Frederic Chopin", "Nocturne Op. 27 No. 1 in C-sharp minor"),
    ("musopen-chopin", "Nocturne Op. 32 no. 1 in B major.mp3", "Frederic Chopin", "Nocturne Op. 32 No. 1 in B major"),
    ("musopen-chopin", "Nocturne Op. 32 no. 2 in A flat major.mp3", "Frederic Chopin", "Nocturne Op. 32 No. 2 in A-flat major"),
    ("musopen-chopin", "Nocturne Op. 48 no. 1 in C minor.mp3", "Frederic Chopin", "Nocturne Op. 48 No. 1 in C minor"),
    ("musopen-chopin", "Nocturne Op. 48 no. 2 in F sharp minor.mp3", "Frederic Chopin", "Nocturne Op. 48 No. 2 in F-sharp minor"),
    ("musopen-chopin", "Nocturne Op. 55 no. 1 in F minor.mp3", "Frederic Chopin", "Nocturne Op. 55 No. 1 in F minor"),
    ("musopen-chopin", "Nocturne Op. 55 no. 2 in E flat major.mp3", "Frederic Chopin", "Nocturne Op. 55 No. 2 in E-flat major"),
    ("musopen-chopin", "Nocturne Op. 62 no. 2 in E major.mp3", "Frederic Chopin", "Nocturne Op. 62 No. 2 in E major"),
    ("musopen-chopin", "NocturneOp.72No.1InEMinor.mp3", "Frederic Chopin", "Nocturne Op. 72 No. 1 in E minor"),
    ("musopen-chopin", "Nocturne B. 108 in C minor.mp3", "Frederic Chopin", "Nocturne B. 108 in C minor"),
    ("musopen-chopin", "Nocturne B. 49 in C sharp minor 'Lento con gran espressione' (1).mp3", "Frederic Chopin", "Nocturne B. 49 in C-sharp minor 'Lento con gran espressione'"),
    ("musopen-chopin", "NocturneOp27No2.mp3", "Frederic Chopin", "Nocturne Op. 27 No. 2 in D-flat major"),

    # ---- Chopin: Mazurkas ----
    ("musopen-chopin", "Mazurka Op. 17 no. 3 in A flat major.mp3", "Frederic Chopin", "Mazurka Op. 17 No. 3 in A-flat major"),
    ("musopen-chopin", "Mazurka Op. 17 no. 4 in A minor.mp3", "Frederic Chopin", "Mazurka Op. 17 No. 4 in A minor"),
    ("musopen-chopin", "Mazurka Op. 24 no. 4 in B flat minor.mp3", "Frederic Chopin", "Mazurka Op. 24 No. 4 in B-flat minor"),
    ("musopen-chopin", "Mazurka Op. 50 no. 3 in C sharp minor.mp3", "Frederic Chopin", "Mazurka Op. 50 No. 3 in C-sharp minor"),
    ("musopen-chopin", "Mazurka Op. 56 no. 1 in B major.mp3", "Frederic Chopin", "Mazurka Op. 56 No. 1 in B major"),
    ("musopen-chopin", "Mazurka Op. 56 no. 3 in C minor.mp3", "Frederic Chopin", "Mazurka Op. 56 No. 3 in C minor"),
    ("musopen-chopin", "Mazurka Op. 59 no. 1 in A minor.mp3", "Frederic Chopin", "Mazurka Op. 59 No. 1 in A minor"),
    ("musopen-chopin", "Mazurka Op. 59 no. 3 in F sharp minor.mp3", "Frederic Chopin", "Mazurka Op. 59 No. 3 in F-sharp minor"),
    ("musopen-chopin", "Mazurka Op. 50 no. 1 in G major.mp3", "Frederic Chopin", "Mazurka Op. 50 No. 1 in G major"),
    ("musopen-chopin", "Mazurka Op. 50 no. 2 in A flat major.mp3", "Frederic Chopin", "Mazurka Op. 50 No. 2 in A-flat major"),
    ("musopen-chopin", "Mazurka Op. 7 no. 3 in F minor.mp3", "Frederic Chopin", "Mazurka Op. 7 No. 3 in F minor"),
    ("musopen-chopin", "Mazurka Op. 24 no. 3 in A flat major.mp3", "Frederic Chopin", "Mazurka Op. 24 No. 3 in A-flat major"),

    # ---- Chopin: Waltzes (calmer, melodic ones) ----
    ("musopen-chopin", "Waltz Op. 64 no. 2 in C sharp minor.mp3", "Frederic Chopin", "Waltz Op. 64 No. 2 in C-sharp minor"),
    ("musopen-chopin", "Waltz Op. 69 no. 1 in A flat major.mp3", "Frederic Chopin", "Waltz Op. 69 No. 1 in A-flat major"),
    ("musopen-chopin", "Waltz Op. 69 no. 2 in B minor.mp3", "Frederic Chopin", "Waltz Op. 69 No. 2 in B minor"),
    ("musopen-chopin", "Waltz Op. 70 no. 2 in F minor.mp3", "Frederic Chopin", "Waltz Op. 70 No. 2 in F minor"),
    ("musopen-chopin", "Waltz Op. 70 no. 3 in D flat major.mp3", "Frederic Chopin", "Waltz Op. 70 No. 3 in D-flat major"),
    ("musopen-chopin", "WaltzOp.34No.2InAMinor.mp3", "Frederic Chopin", "Waltz Op. 34 No. 2 in A minor"),
    ("musopen-chopin", "WaltzB.46InEFlatMajor.mp3", "Frederic Chopin", "Waltz B. 46 in E-flat major"),
    ("musopen-chopin", "WaltzB.56InEMinor.mp3", "Frederic Chopin", "Waltz B. 56 in E minor"),
    ("musopen-chopin", "WaltzOp.34No.3InFMajor.mp3", "Frederic Chopin", "Waltz Op. 34 No. 3 in F major"),
    ("musopen-chopin", "WaltzB.21InAFlatMajor.mp3", "Frederic Chopin", "Waltz B. 21 in A-flat major"),

    # ---- Chopin: Impromptus ----
    ("musopen-chopin", "Fantasie Impromptu Op. 66.mp3", "Frederic Chopin", "Fantaisie-Impromptu Op. 66 in C-sharp minor"),
    ("musopen-chopin", "Impromptu no. 1 - Op. 29.mp3", "Frederic Chopin", "Impromptu No. 1 Op. 29 in A-flat major"),
    ("musopen-chopin", "Impromptu no. 2 - Op. 36.mp3", "Frederic Chopin", "Impromptu No. 2 Op. 36 in F-sharp major"),
    ("musopen-chopin", "Impromptu no. 3 - Op. 51.mp3", "Frederic Chopin", "Impromptu No. 3 Op. 51 in G-flat major"),

    # ---- Chopin: Preludes Op. 28 (calmer ones) ----
    ("musopen-chopin", "Prelude Op. 28 no. 6.mp3", "Frederic Chopin", "Prelude Op. 28 No. 6 in B minor"),
    ("musopen-chopin", "Prelude Op. 28 no. 7.mp3", "Frederic Chopin", "Prelude Op. 28 No. 7 in A major"),
    ("musopen-chopin", "Prelude Op. 28 no. 13.mp3", "Frederic Chopin", "Prelude Op. 28 No. 13 in F-sharp major"),
    ("musopen-chopin", "Prelude Op. 28 no. 15.mp3", "Frederic Chopin", "Prelude Op. 28 No. 15 in D-flat major 'Raindrop'"),
    ("musopen-chopin", "Prelude Op. 28 no. 17.mp3", "Frederic Chopin", "Prelude Op. 28 No. 17 in A-flat major"),

    # ---- Chopin: Cello Sonata Largo ----
    ("musopen-chopin", "Sonata for Piano and Cello in G Minor, Op. 65 - III. Largo.mp3", "Frederic Chopin", "Sonata for Piano and Cello, Op. 65 - III. Largo"),
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


def download_classic_expansion():
    """Download the IA_CLASSIC_EXPANSION batch into tmp/classic/ (skipping any
    files already present locally or already represented in classic.m3u)."""
    out_dir = MUSIC_DIR / "classic"
    out_dir.mkdir(parents=True, exist_ok=True)

    existing_in_m3u = m3u_existing_container_paths("classic")

    print(f"\n{'='*60}")
    print(f"  Downloading classical expansion ({len(IA_CLASSIC_EXPANSION)} tracks)")
    print(f"{'='*60}")

    downloaded = 0
    for i, (identifier, relative_path, artist, title) in enumerate(IA_CLASSIC_EXPANSION, start=1):
        out_path = manifest_output_path("classic", artist, title)
        container_path = f"/music/classic/{out_path.name}"
        if container_path in existing_in_m3u:
            print(f"  [skip-m3u] {artist} - {title}")
            continue
        if out_path.exists():
            print(f"  [skip-file] {artist} - {title}")
            continue

        print(f"  [{i}/{len(IA_CLASSIC_EXPANSION)}] {artist} - {title}")
        try:
            dl_url = f"https://archive.org/download/{identifier}/{urllib.request.quote(relative_path)}"
            urllib.request.urlretrieve(dl_url, str(out_path))
            downloaded += 1
        except Exception as e:
            print(f"  [error] {e}")
            if out_path.exists():
                out_path.unlink()

    print(f"  Downloaded {downloaded} new tracks to {out_dir}")


def m3u_existing_container_paths(genre: str) -> set:
    """Parse an existing .m3u and return the set of container paths already
    represented (the part after the final ':' on annotate lines)."""
    m3u_path = LIQUIDSOAP_DIR / f"{genre}.m3u"
    if not m3u_path.exists():
        return set()
    paths = set()
    for line in m3u_path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        # annotate:...:/music/<genre>/<file>
        marker = f":/music/{genre}/"
        idx = line.find(marker)
        if idx >= 0:
            paths.add(line[idx + 1:])
        elif line.startswith(f"/music/{genre}/"):
            paths.add(line)
    return paths


def append_classic_expansion_to_m3u():
    """Append annotate lines for any IA_CLASSIC_EXPANSION tracks present in
    tmp/classic/ but not yet in classic.m3u. Existing entries are untouched."""
    m3u_path = LIQUIDSOAP_DIR / "classic.m3u"
    existing_paths = m3u_existing_container_paths("classic")

    new_lines = []
    skipped_missing = 0
    for identifier, relative_path, artist, title in IA_CLASSIC_EXPANSION:
        mp3 = manifest_output_path("classic", artist, title)
        container_path = f"/music/classic/{mp3.name}"
        if container_path in existing_paths:
            continue
        if not mp3.exists():
            skipped_missing += 1
            continue

        duration = ""
        try:
            result = subprocess.run(
                ["ffprobe", "-v", "quiet", "-print_format", "json",
                 "-show_format", str(mp3)],
                capture_output=True, text=True, timeout=5,
            )
            if result.returncode == 0:
                fmt = json.loads(result.stdout).get("format", {})
                dur_secs = float(fmt.get("duration", 0))
                if dur_secs > 0:
                    duration = str(int(dur_secs))
        except Exception:
            pass

        # liquidsoap annotate format expects double-quoted values; embed any
        # apostrophes literally and replace any double quotes in source with
        # single quotes (matches the existing manifest convention)
        a = artist.replace('"', "'")
        t = title.replace('"', "'")
        dur_part = f',duration="{duration}"' if duration else ""
        new_lines.append(f'annotate:artist="{a}",title="{t}"{dur_part}:{container_path}')

    if not new_lines:
        print(f"  No new tracks to append to {m3u_path.name} ({skipped_missing} missing locally)")
        return

    # Append, ensuring the existing file ends with a newline
    existing_content = m3u_path.read_text() if m3u_path.exists() else ""
    if existing_content and not existing_content.endswith("\n"):
        existing_content += "\n"
    m3u_path.write_text(existing_content + "\n".join(new_lines) + "\n")
    print(f"  Appended {len(new_lines)} tracks to {m3u_path.name} (skipped {skipped_missing} missing locally)")


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
                        help="Where to store downloaded music (default: repo tmp/, the upload-staging area)")
    parser.add_argument("--liquidsoap-dir", type=Path, default=DEFAULT_LIQUIDSOAP_DIR,
                        help="Where to write generated .m3u files (default: repo infra/liquidsoap/)")
    parser.add_argument("--m3u-only", action="store_true",
                        help="Only regenerate .m3u files from existing downloads")
    parser.add_argument("--skip-m3u", action="store_true",
                        help="Skip generating .m3u files")
    parser.add_argument("--classic-expand", action="store_true",
                        help="Download the IA_CLASSIC_EXPANSION batch into tmp/classic/ "
                             "and append the new tracks to classic.m3u (existing entries "
                             "are preserved). Skips all other genres and download paths.")
    args = parser.parse_args()

    MUSIC_DIR = args.music_dir.resolve()
    LIQUIDSOAP_DIR = args.liquidsoap_dir.resolve()

    if args.classic_expand:
        download_classic_expansion()
        if not args.skip_m3u:
            print(f"\n{'='*60}")
            print("  Appending new classical tracks to classic.m3u")
            print(f"{'='*60}")
            append_classic_expansion_to_m3u()
        print("\nDone! Next steps:")
        print(f"  1. Review {LIQUIDSOAP_DIR}/classic.m3u (new entries appended at the end)")
        print(f"  2. Upload tmp/classic/*.mp3 to R2, then clear tmp/classic/")
        return

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
