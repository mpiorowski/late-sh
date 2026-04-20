# Music Inventory

This file tracks the local music catalog used by `late.sh` radio.

- Runtime source of truth for playback order is the `.m3u` files in `infra/liquidsoap/`.
- Source of truth for reproducible fetching is `scripts/fetch_cc_music.py` plus `scripts/fetch_ambient_refresh.py` for the expanded ambient catalog.
- `CONTEXT.md` should keep only high-signal status and point here for detailed track inventories.

## Library Status

- `lofi`: done, 50 tracks, mixed `CC0` and `CC-BY 4.0`
- `ambient`: done, 93 tracks, mixed `CC0` and `CC-BY 4.0`
- `classic`: done, 40 tracks, public domain via Musopen / Internet Archive
- `jazz`: pending

## Lofi

| # | Artist | Title | License | Source URL |
|---|--------|-------|---------|------------|
| 1 | HoliznaCC0 | A Little Shade | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 2 | HoliznaCC0 | All The Way Sad | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 3 | HoliznaCC0 | Autumn | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 4 | HoliznaCC0 | Cellar Door | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 5 | HoliznaCC0 | Everything You Ever Dreamed | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 6 | HoliznaCC0 | Foggy Headed | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 7 | HoliznaCC0 | Ghosts | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 8 | HoliznaCC0 | Glad To Be Stuck Inside | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 9 | HoliznaCC0 | Laundry Day | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 10 | HoliznaCC0 | Letting Go Of The Past | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 11 | HoliznaCC0 | Lighter Than Air | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 12 | HoliznaCC0 | Limbo | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 13 | HoliznaCC0 | Lofi Forever | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 14 | HoliznaCC0 | Morning Coffee | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 15 | HoliznaCC0 | Mundane | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 16 | HoliznaCC0 | Pretty Little Lies | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 17 | HoliznaCC0 | Seasons Change | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 18 | HoliznaCC0 | Shut Up Or Shut In | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 19 | HoliznaCC0 | Small Towns, Smaller Lives | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 20 | HoliznaCC0 | Something In The Air | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 21 | HoliznaCC0 | Static | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 22 | HoliznaCC0 | Vintage | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 23 | HoliznaCC0 | Whatever... | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 24 | HoliznaCC0 | Yesterday | CC0 | https://holiznacc0.bandcamp.com/album/lofi-and-chill |
| 25 | JMHBM | Mirrors | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/mirrors-2/ |
| 26 | Ketsa | Deep Waves | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/deep-waves/ |
| 27 | Ketsa | Lofi Chill Hop - 20 Min Mix | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-hip-hop-mixes/lofi-chill-hop-20-min-mix/ |
| 28 | Ketsa | Lofi Soul Hip Hop Mix 3 | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-hip-hop-mixes/lofi-soul-hip-hop-mix-3/ |
| 29 | Ketsa | Shining Still | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/shining-still/ |
| 30 | Pufino | Vibing | CC-BY 4.0 | https://freemusicarchive.org/music/pufino/single/vibing-chill-lofi-royalty-free-music/ |
| 31 | snoozy beats | lofi mood | CC-BY 4.0 | https://freemusicarchive.org/music/snoozy-beats/single/lofi-mood-chill-lofi/ |
| 32 | snoozy beats | lovely days | CC-BY 4.0 | https://freemusicarchive.org/music/snoozy-beats/single/lovely-days-lofi/ |
| 33 | snoozy beats | moon beat | CC-BY 4.0 | https://freemusicarchive.org/music/snoozy-beats/single/moon-beat-lofi/ |
| 34 | snoozy beats | morning dew | CC-BY 4.0 | https://freemusicarchive.org/music/snoozy-beats/single/morning-dew-chill-lofi/ |
| 35 | snoozy beats | pink ocean | CC-BY 4.0 | https://freemusicarchive.org/music/snoozy-beats/single/pink-ocean-chill-lofi-beatmp3/ |
| 36 | YuraSoop | Chill Lo-Fi | CC-BY 4.0 | https://freemusicarchive.org/music/yurasoop/single/chill-lo-fi/ |
| 37 | Ketsa | I Dream Of You | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/i-dream-of-you/ |
| 38 | Ketsa | Black Screen | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/black-screen/ |
| 39 | Ketsa | Slow Dance | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/slow-dance/ |
| 40 | Ketsa | Seconds Left | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/seconds-left/ |
| 41 | Ketsa | Lowest Sun | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/lowest-sun/ |
| 42 | Ketsa | Reclaimed | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/reclaimed/ |
| 43 | Ketsa | The Time It Takes | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/the-time-it-takes/ |
| 44 | JMHBM | One More | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/one-more/ |
| 45 | JMHBM | Night City | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/night-city/ |
| 46 | JMHBM | New New | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/new-new/ |
| 47 | JMHBM | Do Me Right | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/do-me-right/ |
| 48 | JMHBM | Heavyweights | CC-BY 4.0 | https://freemusicarchive.org/index.php/music/beat-mekanik/single/heavyweights/ |
| 49 | JMHBM | Footsteps | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/footsteps/ |
| 50 | legacyAlli | RF - LoFi Funky and Chunky | CC-BY 4.0 | https://freemusicarchive.org/music/legacyalli/instrumental-by-legacyalli-2024/rf-lofi-funky-and-chunky/ |

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

| # | Artist | Title | License | Source URL |
|---|--------|-------|---------|------------|
| 1 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Aria | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 2 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 5 | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 3 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 13 | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 4 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Aria Da Capo | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 5 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - I. Allegro con brio | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 6 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - II. Adagio ma non troppo | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 7 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - III. Scherzo Allegro | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 8 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - IV. La Malinconia | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 9 | Alexander Borodin | String Quartet No. 2 in D Major - I. Allegro moderato | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 10 | Alexander Borodin | String Quartet No. 2 in D Major - II. Scherzo Allegro | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 11 | Alexander Borodin | String Quartet No. 2 in D Major - III. Nocturne Andante | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 12 | Alexander Borodin | String Quartet No. 2 in D Major - IV. Finale Andante - Vivace | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 13 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - I. Allegro ma non troppo | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 14 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - II. Lento | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 15 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - III. Molto Vivace | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 16 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - IV. Finale Vivace ma non troppo | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 17 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - I. Allegro moderato | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 18 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - II. Adagio cantabile | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 19 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - III. Menuetto Allegretto | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 20 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - IV. Finale Vivace | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 21 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - I. Adagio Allegro | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 22 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - II. Andante cantabile | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 23 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - III. Minuetto Allegretto | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 24 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - IV. Allegro molto | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 25 | Franz Schubert | Sonata in A Major, D. 664 - I. Allegro moderato | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 26 | Franz Schubert | Sonata in A Major, D. 664 - II. Andante | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 27 | Franz Schubert | Sonata in A Major, D. 664 - III. Allegro | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 28 | Franz Schubert | Sonata in E-flat Major, D. 568 - I. Allegro moderato | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 29 | Franz Schubert | Sonata in E-flat Major, D. 568 - II. Andante molto | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 30 | Franz Schubert | Sonata in E-flat Major, D. 568 - III. Menuetto Allegretto | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 31 | Franz Schubert | Sonata in E-flat Major, D. 568 - IV. Allegro moderato | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 32 | Johannes Brahms | Symphony No. 2 in D Major, Op. 73 - I. Allegro non troppo | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 33 | Johannes Brahms | Symphony No. 2 in D Major, Op. 73 - II. Adagio non troppo | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 34 | Johannes Brahms | Symphony No. 2 in D Major, Op. 73 - III. Allegretto grazioso | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 35 | Josef Suk | Meditation | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 36 | Alexander Borodin | In the Steppes of Central Asia | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 37 | Felix Mendelssohn | Hebrides Overture 'Fingal's Cave' | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 38 | Bedrich Smetana | Ma Vlast - Vltava | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 39 | Wolfgang Amadeus Mozart | Magic Flute Overture | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |
| 40 | Ludwig van Beethoven | Egmont Overture, Op. 84 | Public Domain | https://archive.org/details/MusopenCollectionAsFlac |

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
