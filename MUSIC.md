# Music Inventory

This file tracks the local music catalog used by `late.sh` radio.

- Runtime source of truth for playback order is the `.m3u` files in `infra/liquidsoap/`.
- Source of truth for reproducible fetching is `scripts/fetch_cc_music.py` plus `scripts/fetch_ambient_refresh.py` for the expanded ambient catalog.
- `CONTEXT.md` should keep only high-signal status and point here for detailed track inventories.

## Library Status

- `lofi`: done, 100-track manifest, mixed `CC0` and `CC-BY 4.0`
- `ambient`: done, 93 tracks, mixed `CC0` and `CC-BY 4.0`
- `classic`: done, 100-track calm-first manifest, public domain via Musopen / Internet Archive
- `jazz`: pending

## Lofi

This section documents the current 100-track lofi manifest used by the regenerated playlist files. The dev Liquidsoap stack now mounts `tmp/music/lofi` onto `/music/lofi`, so the local runtime playlist resolves against the refreshed temp library.

### HoliznaCC0 - Lofi And Chill

- Count: 24
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/lofi-and-chill
- Tracks: A Little Shade; All The Way Sad; Autumn; Cellar Door; Everything You Ever Dreamed; Foggy Headed; Ghosts; Glad To Be Stuck Inside; Laundry Day; Letting Go Of The Past; Lighter Than Air; Limbo; Lofi Forever; Morning Coffee; Mundane; Pretty Little Lies; Seasons Change; Shut Up Or Shut In; Small Towns, Smaller Lives; Something In The Air; Static; Vintage; Whatever...; Yesterday

### HoliznaCC0 - Public Domain Lo-fi

- Count: 29
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/public-domain-lo-fi
- Tracks: Bubbles; Calm Current; Color Of A Soul; Complicated Feelings; Dream shifter; Dreamy Reverie; Ease Into Night; Infinite Echoes; Into The Mist; Lucid; Never Sleeping; Ode To Forgetting; Peaceful Drift; Reminders; Saturation; Walking Away; Wave Maker; Wetlands; Canon Event; Moon Unit; One Night In France; Still Life; Theta Frequency; Tokyo Sunset; Tranquil Mindset; Blue Skies; laundry On The Wire; Waves; Windows Down

### HoliznaCC0 - Winter Lo-Fi

- Count: 5
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/winter-lo-fi-2
- Tracks: First Snow; Snow Drift; 2 Hour Delay; Fire Place; Winter Blues

### HoliznaCC0 - City Slacker

- Count: 4
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/city-slacker
- Tracks: Busking In The SunLight; Bus Stop; Busted Ac Unit; Nowhere To Be, Nothing To Do

### Ketsa - Lofi Downtempo

- Count: 13
- License: CC-BY 4.0
- Source: https://freemusicarchive.org/music/Ketsa/lofi-downtempo
- Tracks: Tetra; I Dream Of You; Black Screen; Slow Dance; Seconds Left; Lowest Sun; Down Pitch; Reclaimed; The Time It Takes; Deep Waves; Shining Still; The Winter Months; Folded

### Ketsa - Vintage Beats

- Count: 18
- License: CC-BY 4.0
- Source: https://freemusicarchive.org/music/Ketsa/vintage-beats
- Tracks: Home Sigh; Take Me Up; Appointments; Jazz Daze; Bring Dat; Make Me Sad; In Trouble; World's A Stage; Smoothness; Journal; My Biz; Aligning Frequencies; Therapy; Sun Slides; To do; Grand Rising; The Cure; Keep Hold

### Beat Mekanik - Singles

- Count: 6
- License: CC-BY 4.0
- Source: https://freemusicarchive.org/music/beat-mekanik/single/
- Tracks: One More; Night City; New New; Do Me Right; Heavyweights; Footsteps

### legacyAlli - Single

- Count: 1
- License: CC-BY 4.0
- Source: https://freemusicarchive.org/music/legacyalli/instrumental-by-legacyalli-2024/rf-lofi-funky-and-chunky/
- Tracks: RF - LoFi Funky and Chunky

## Ambient

| # | Artist | Title | License | Source URL |
|---|--------|-------|---------|------------|
| 1 | 1000 Handz | Alchemist | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/alchemist/ |
| 2 | 1000 Handz | Astral Longing | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/astral-longing/ |
| 3 | 1000 Handz | Astral | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/astral-1/ |
| 4 | 1000 Handz | Avatar | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/avatar/ |
| 5 | 1000 Handz | Cosmos | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodic-rap-instrumentals-vol-2/cosmos-3/ |
| 6 | 1000 Handz | Cross Rhodes | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/cross-rhodes/ |
| 7 | 1000 Handz | Dance Hall | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/dance-hall/ |
| 8 | 1000 Handz | Dark Side of the Moon | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodic-rap-instrumentals-vol-2/dark-side-of-the-moon-1/ |
| 9 | 1000 Handz | Download | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/download/ |
| 10 | 1000 Handz | Galactic | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-electronicgaming-instrumentals/galactic/ |
| 11 | 1000 Handz | Giza | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/giza-2/ |
| 12 | 1000 Handz | Guild | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/guild/ |
| 13 | 1000 Handz | Hopeful | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/hopeful-3/ |
| 14 | 1000 Handz | Isles | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/isles/ |
| 15 | 1000 Handz | Kraken | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-electronicgaming-instrumentals/kraken/ |
| 16 | 1000 Handz | Lilies | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/lilies/ |
| 17 | 1000 Handz | Magneto | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/magneto/ |
| 18 | 1000 Handz | Misunderstood | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/misunderstood-4/ |
| 19 | 1000 Handz | Monaco | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/monaco/ |
| 20 | 1000 Handz | Motherboard | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-electronicgaming-instrumentals/motherboard-1/ |
| 21 | 1000 Handz | Mystery | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/mystery-2/ |
| 22 | 1000 Handz | Orbitol | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/orbitol/ |
| 23 | 1000 Handz | Orion (no drums) | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/orion-no-drums/ |
| 24 | 1000 Handz | Phantomm | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-electronicgaming-instrumentals/phantomm/ |
| 25 | 1000 Handz | Potential | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/potential/ |
| 26 | 1000 Handz | Saturn ft. ADG | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-electronicgaming-instrumentals/saturn-ft-adg/ |
| 27 | 1000 Handz | Shatter | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodic-rap-instrumentals-vol-2/shatter-1/ |
| 28 | 1000 Handz | Silense | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/silense/ |
| 29 | 1000 Handz | Stories | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/stories-2/ |
| 30 | 1000 Handz | Tea | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/tea/ |
| 31 | 1000 Handz | The Muse | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/the-muse/ |
| 32 | 1000 Handz | The Shire | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/the-shire/ |
| 33 | 1000 Handz | The Well | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/the-well/ |
| 34 | 1000 Handz | Through The Stars | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/through-the-stars-1/ |
| 35 | 1000 Handz | Throughout | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/throughout/ |
| 36 | 1000 Handz | Tundra | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/tundra/ |
| 37 | 1000 Handz | Unlimited | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-electronicgaming-instrumentals/unlimited/ |
| 38 | 1000 Handz | Wednesday | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-ambientbackground-scores/wednesday-1/ |
| 39 | 1000 Handz | World Is Yourz | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/world-is-yourz/ |
| 40 | 1000 Handz | Xperience | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-melodiessamples-no-drums/xperience/ |
| 41 | Holizna (Synthetic People) | A Lonely Asteroid Headed Towards Earth | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 42 | Holizna (Synthetic People) | A Small Town On Pluto (Family Vacation) | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 43 | Holizna (Synthetic People) | A Small Town On Pluto (The Grocery Store) | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 44 | Holizna (Synthetic People) | Astronaut (Part 2) | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 45 | Holizna (Synthetic People) | Astronaut (Part 3) | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 46 | Holizna (Synthetic People) | Astronaut | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 47 | Holizna (Synthetic People) | Before The Big Bang | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 48 | Holizna (Synthetic People) | Fomalhaut b, Iota Draconis-b, Mu Arae c, WASP 17b, and 51 Pegasi b, This is for You! | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 49 | Holizna (Synthetic People) | Saturn In A Meteor Shower | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 50 | Holizna (Synthetic People) | Space Hospitals | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 51 | Holizna (Synthetic People) | The Milky Way | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 52 | Holizna (Synthetic People) | Tiny Plastic Video Games For Long Anxious Space Travel | CC0 | https://holiznacc0.bandcamp.com/album/an-ocean-in-outer-space |
| 53 | Holizna | A Cloud Dog Named Sky | CC0 | https://holiznacc0.bandcamp.com/album/make-shift-salvation |
| 54 | Holizna | A Small Town On Pluto | CC0 | https://holiznacc0.bandcamp.com/album/a-small-town-on-pluto |
| 55 | Holizna | Cold Feet | CC0 | https://holiznacc0.bandcamp.com/album/a-small-town-on-pluto |
| 56 | Holizna | Goodbye Good Times | CC0 | https://holiznacc0.bandcamp.com/album/make-shift-salvation |
| 57 | Holizna | Iron Skies | CC0 | https://holiznacc0.bandcamp.com/album/make-shift-salvation |
| 58 | Holizna | Last Train To Earth | CC0 | https://holiznacc0.bandcamp.com/album/a-small-town-on-pluto |
| 59 | Holizna | Make-Shift Salvation | CC0 | https://holiznacc0.bandcamp.com/album/make-shift-salvation |
| 60 | Holizna | The Edge Of Nowhere | CC0 | https://holiznacc0.bandcamp.com/album/make-shift-salvation |
| 61 | Holizna | The Only Store In Town | CC0 | https://holiznacc0.bandcamp.com/album/a-small-town-on-pluto |
| 62 | Holizna | The Wind That Whistled Through The Wicker Chair | CC0 | https://holiznacc0.bandcamp.com/album/make-shift-salvation |
| 63 | Almusic34 | Deep Space Ambient | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/deep-space-ambientmp3/ |
| 64 | Almusic34 | Space Ambient Mix 4 | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/space-ambient-mix-4mp3/ |
| 65 | Almusic34 | Space Ambient Mix | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/space-ambient-mixmp3 |
| 66 | Amarent | A Better World | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/a-better-world/ |
| 67 | Amarent | At the Heart of It Is Just Me and You (Instrumental) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/instrumental-versions/at-the-heart-of-it-is-just-me-and-you-instrumental/ |
| 68 | Amarent | Cathay Lounge | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/cathay-lounge/ |
| 69 | Amarent | Ethereal | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-atmospheric-music/ethereal-2/ |
| 70 | Amarent | Never Let Go (Instrumental) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/instrumental-versions/never-let-go-instrumental/ |
| 71 | Amarent | Outer Space | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-atmospheric-music/outer-space/ |
| 72 | Amarent | Salt Lake Swerve (Chillout Remix) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/salt-lake-swerve-chillout-remix/ |
| 73 | Amarent | Sweet Dreams (Middle-Eastern Remix) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/sweet-dreams-middle-eastern-remix/ |
| 74 | Amarent | Sweet Dreams | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/sweet-dreams-2/ |
| 75 | Amarent | Sweet Love (Chill Remix) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/sweet-love-chill-remix/ |
| 76 | Amarent | Swirling Snowflakes - Finale | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-ambient-music/swirling-snowflakes-finale/ |
| 77 | Amarent | To the Moon (Instrumental) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/instrumental-versions/to-the-moon-instrumental/ |
| 78 | Amarent | Tuesday Night (Radio Edit) | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-atmospheric-music/tuesday-night-radio-edit/ |
| 79 | Amarent | Tuesday Night | CC-BY 4.0 | https://freemusicarchive.org/music/amarent/free-atmospheric-music/tuesday-night/ |
| 80 | Ketsa | Around the Corner | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/around-the-corner/ |
| 81 | Ketsa | Harmony | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/harmony-4/ |
| 82 | Ketsa | Machine Ghosts | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/machine-ghosts/ |
| 83 | Ketsa | Meditation | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/modern-meditations/meditation-5/ |
| 84 | Ketsa | Morning Stillness | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/modern-meditations/morning-stillness/ |
| 85 | Ketsa | Patterns | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/modern-meditations/patterns-1/ |
| 86 | Ketsa | Still Dreams | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/still-dreams/ |
| 87 | Ketsa | Surroundings are Green | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/surroundings-are-green/ |
| 88 | Ketsa | Where Dreams Drift | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/where-dreams-drift/ |
| 89 | Sergey Cheremisinov | Last Moon Last Stars | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/metamorphoses/last-moon-last-stars/ |
| 90 | Sergey Cheremisinov | Metamorphoses | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/metamorphoses/metamorphoses/ |
| 91 | Sergey Cheremisinov | Mindful Choice | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/metamorphoses/mindful-choice/ |
| 92 | Splashkabona | Dreamy Ambient Positive Moments in Time | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/dreamy-ambient-positive-moments-in-time/ |
| 93 | Vlad Annenkov | Emotional Cinematic Ambient "Gentle Memory" | CC-BY 4.0 | https://freemusicarchive.org/music/vlad-annenkov/single/emotional-cinematic-ambient-gentle-memorymp3/ |

## Classic

This section documents the current 100-track calm-first classical manifest used by the regenerated playlist files. The dev Liquidsoap stack mounts `tmp/music/classic` onto `/music/classic`, so the local runtime playlist resolves against the refreshed temp library.

### Johann Sebastian Bach - Goldberg Variations, BWV 988

- Count: 32
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: Aria; Variation 1; Variation 2; Variation 3. Canon on the unison; Variation 4; Variation 5; Variation 6. Canon on the second; Variation 7; Variation 8; Variation 9. Canon on the third; Variation 10. Fughetta; Variation 11; Variation 12. Canon on the fourth; Variation 13; Variation 14; Variation 15. Canon on the fifth; Variation 16. Overture; Variation 17; Variation 18. Canon on the sixth; Variation 19; Variation 20; Variation 21. Canon on the seventh; Variation 22; Variation 23; Variation 24. Canon on the octave; Variation 25; Variation 26; Variation 27. Canon on the ninth; Variation 28; Variation 29; Variation 30. Quodlibet; Aria Da Capo

### Ludwig van Beethoven - String Quartet No. 6 in B-flat Major, Op. 18 No. 6

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro con brio; II. Adagio ma non troppo; III. Scherzo Allegro; IV. La Malinconia

### Wolfgang Amadeus Mozart - String Quartet No. 15 in D Minor, K. 421

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro moderato; II. Andante; III. Minuetto; IV. Allegro ma non troppo

### Ludwig van Beethoven - Symphony No. 3 in E Flat Major "Eroica", Op. 55

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 02 - Marcia funebre Adagio assai

### Alexander Borodin - String Quartet No. 1 in A Major

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 01 - Moderato - Allegro; 02 - Andante con moto; 04 - Andante - Allegro risoluto

### Alexander Borodin - String Quartet No. 2 in D Major

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro moderato; II. Scherzo Allegro; III. Nocturne Andante; IV. Finale Andante - Vivace

### Franz Schubert - Sonata in A Minor, D. 845

- Count: 2
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Moderato; II. Andante poco mosso

### Johannes Brahms - Symphony No. 1 in C Minor, Op. 68

- Count: 2
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 02 - Andante sostenuto; 03 - Un poco allegretto e grazioso

### Johannes Brahms - Symphony No. 3 in F Major, Op. 90

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 01 - Allegro con brio; 02 - Andante; 03 - Poco allegretto; 04 - Allegro

### Johannes Brahms - Symphony No. 4 in E Minor, Op. 98

- Count: 2
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 01 - Allegro Non Troppo; 02 - Andante Moderato

### Franz Schubert - Sonata in A Minor, D. 959

- Count: 2
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: II. Andantino; IV. Rondo Allegretto

### Antonin Dvorak - String Quartet No. 12 in F Major, Op. 96 'American'

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro ma non troppo; II. Lento; IV. Finale Vivace ma non troppo

### Franz Schubert - Sonata in C Minor, D. 958

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: II. Adagio

### Antonin Dvorak - String Quartet No. 10 in E Flat, Op. 51

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 01 - Allegro Ma Non Troppo; 02 - Dumka; 03 - Romanza

### Franz Schubert - Sonata in A Minor, D. 784

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: II. Andante

### Edvard Grieg - Peer Gynt Suite No. 1, Op. 46

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 01 - Morning; 02 - Aase's Death; 03 - Anitra's Dream

### Felix Mendelssohn - Symphony No. 3 in A Minor 'Scottish', Op. 56

- Count: 2
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Andante con moto; III. Adagio

### Joseph Haydn - String Quartet in D Major, Op. 64 No. 5 'Lark'

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro moderato; II. Adagio cantabile; III. Menuetto Allegretto; IV. Finale Vivace

### Felix Mendelssohn - Symphony No. 4 in A Major, Op. 90 'Italian'

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: 01 - Allegro vivace; 02 - Andante con moto; 03 - Con moto moderato

### Wolfgang Amadeus Mozart - String Quartet No. 19 in C Major, K. 465 'Dissonance'

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Adagio Allegro; II. Andante cantabile; III. Minuetto Allegretto; IV. Allegro molto

### Franz Schubert - Sonata in A Major, D. 664

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro moderato; II. Andante; III. Allegro

### Franz Schubert - Sonata in E-flat Major, D. 568

- Count: 4
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro moderato; II. Andante molto; III. Menuetto Allegretto; IV. Allegro moderato

### Johannes Brahms - Symphony No. 2 in D Major, Op. 73

- Count: 3
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: I. Allegro non troppo; II. Adagio non troppo; III. Allegretto grazioso

### Josef Suk - Meditation

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: Meditation

### Alexander Borodin - In the Steppes of Central Asia

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: In the Steppes of Central Asia

### Felix Mendelssohn - Hebrides Overture 'Fingal's Cave'

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: Hebrides Overture 'Fingal's Cave'

### Bedrich Smetana - Ma Vlast - Vltava

- Count: 1
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: Ma Vlast - Vltava

### Wolfgang Amadeus Mozart - Symphony No. 40 in G Minor, K. 550

- Count: 2
- License: Public Domain
- Source: https://archive.org/details/MusopenCollectionAsFlac
- Tracks: II. Andante; III. Menuetto Allegretto

## Planned Sources

### Jazz

- HoliznaCC0, `Busted Guitar Jazz`:
  https://holiznacc0.bandcamp.com/album/lofi-jazz-guitar
- Kevin MacLeod, `Jazz Sampler`:
  https://archive.org/details/Jazz_Sampler-9619
- Kevin MacLeod, `Jazz & Blues`:
  https://kevinmacleod1.bandcamp.com/album/jazz-blues
- Ketsa, `CC BY: FREE TO USE FOR ANYTHING`:
  https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything
