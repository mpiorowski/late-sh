# Music Inventory

This file tracks the local music catalog used by `late.sh` radio.

- Runtime source of truth for playback order is the `.m3u` files in `infra/liquidsoap/`.
- Source of truth for reproducible fetching is `scripts/fetch_cc_music.py` plus `scripts/fetch_ambient_refresh.py` for the expanded ambient catalog.
- `CONTEXT.md` should keep only high-signal status and point here for detailed track inventories.

## Library Status

- `lofi`: done, 202-track manifest, mixed `CC0` and `CC-BY 4.0`
- `ambient`: done, 204 tracks, mixed `CC0` and `CC-BY 4.0`
- `classic`: done, 100-track calm-first manifest, public domain via Musopen / Internet Archive
- `jazz`: pending

## Lofi

This section documents the current 202-track lofi manifest used by the regenerated playlist files. The dev Liquidsoap stack now mounts `tmp/music/lofi` onto `/music/lofi`, so the local runtime playlist resolves against the refreshed temp library.

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

### HoliznaCC0 - Only In The Milky Way

- Count: 8
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/only-in-the-milky-way
- Tracks: Anxiety; Boredom; Deja Vu; Love; Memories; Childhood; Dancing; Day Jobs

### HoliznaCC0 - We Drove All Night

- Count: 5
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/we-drove-all-night
- Tracks: City In The Rearview; I Thought You Were Cool; Quiet Moonlit Countrysides; Stealing Glimpses Of Your Face; Morning Light

### HoliznaCC0 - Bassic

- Count: 5
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/bassic
- Tracks: Eat; Sleep; Breath; Make Money; Make Love

### HoliznaCC0 - Gamer Beats!

- Count: 3
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/gamer-beats
- Tracks: Final Level; Coins; Legends

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

### HoliznaCC0 - Waves Of Nostalgia Part 2

- Count: 9
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2
- Tracks: Machines With Feelings; All The Fight Left; New Gods; Cyber Anxiety; Night Life; Fires Uptown; Street Lights Passing By; Internal Panic; We Used To Dance

### HoliznaCC0 - Eternal Skies (Retro Gamer)

- Count: 22
- License: CC0
- Source: https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer
- Tracks: Comfort Game #1; Comfort Game #2; Comfort Game #3; Comfort Game #4; Trees In The Fog; Flying; Half Machine; Home; Jump; Mini Boss; Magic Orb; Mystery; A Fight In The Dark; Quickly!; A Hero Is Born; Random Encounter; Bag Of Carrying; Righteous Sword; City Limits; Secret Map; Credits; Skyline

### HoliznaCC0 - Background Music

- Count: 20
- License: CC0
- Source: https://freemusicarchive.org/music/holiznacc0/background-music/
- Tracks: OST Music Box 1; OST Music Box 2; OST Music Box 3; OST Music Box 4; OST Music Box 5; OST Music Box 6; OST Music Box 7; Drifting Piano; A Small Town On Pluto (Music Box); A Small Town On Pluto (Composed); Game Travel 1 (Piano); VST Guitar; Cabin Fever; Spring On The Horizon; Creepy Piano 1; Creepy Piano 2; Creepy Piano 3; Creepy Piano 4; Dangerous Voyage; Dangerous Voyage (Music Box)

### Ketsa - CC BY: Free To Use For Anything (selected calm picks)

- Count: 30
- License: CC-BY 4.0
- Source: https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything
- Tracks: Saviour Above; Always Faithful; All Ways; Feeling; Importance; Trench Work; Night Flow Day Grow; Will Make You Happy; Bright State; Cello; Dry and High; The Road; Kinship; Her Memory Fading; What It Feels Like; A Little Faith; Tide Turns; Longer Wait; Life is Great; That Feeling; London West; Dawn Faded; Good Feel; Here For You; Brazilian Sunsets; Too Late; The Road 2; Off Days; Inside Dead; Vision

## Ambient

This section documents the current 204-track ambient manifest used by the regenerated playlist files.

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
| 94 | Almusic34 | Energetic Transition | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/energetic-transitionmp3/ |
| 95 | Almusic34 | Other World 1 | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/other-world-1mp3/ |
| 96 | Almusic34 | Call of the Wind Spirits | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/call-of-the-wind-spiritsmp3/ |
| 97 | Almusic34 | Sea and Birds | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/sea-and-birdsmp3-1/ |
| 98 | Almusic34 | Quiet Space | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/quiet-spacemp3/ |
| 99 | Almusic34 | Wind Chimes and Birds | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/wind-chimes-and-birdsmp3-1/ |
| 100 | Almusic34 | Crystal Chamber 1 | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/crystal-chamber-1mp3-1/ |
| 101 | Almusic34 | The Majestic Ocean | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/the-majestic-oceanmp3/ |
| 102 | Almusic34 | Harmony in the Night | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/harmony-in-the-nightmp3-1/ |
| 103 | Almusic34 | Peace Landscape 3 | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/peace-landscape-3mp3-1/ |
| 104 | Almusic34 | Voices and Bells | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/voices-and-bellsmp3-1/ |
| 105 | Almusic34 | Tranquility | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/tranquilitymp3-2/ |
| 106 | Almusic34 | Flute in the Wind | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/flute-in-the-windmp3/ |
| 107 | Almusic34 | Sound Reflections | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/sound-reflectionsmp3/ |
| 108 | Almusic34 | Voices in the Wind | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/voices-in-the-windmp3/ |
| 109 | Almusic34 | Night of Peace | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/night-of-peacemp3-1/ |
| 110 | Almusic34 | Wind and Crystals | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/wind-and-crystalsmp3/ |
| 111 | Almusic34 | Flutes in Peace | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/flutes-in-peacemp3-1/ |
| 112 | Almusic34 | Deep Space Travel | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/deep-space-travelmp3-1/ |
| 113 | Almusic34 | Wind Chimes Harmony | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/wind-chimes-harmonymp3-1/ |
| 114 | Almusic34 | Meditative Flute | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/meditative-flutemp3/ |
| 115 | Almusic34 | Peace in the Light | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/peace-in-the-lightmp3/ |
| 116 | Almusic34 | Landscape of Peace | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/landscape-of-peacemp3/ |
| 117 | Almusic34 | Resonances | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/resonancesmp3/ |
| 118 | Almusic34 | Mysterious Flute | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/mysterious-flutemp3/ |
| 119 | Almusic34 | Flute and Windchimes | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/flute-and-windchimesmp3-1/ |
| 120 | Almusic34 | Nature Spirits | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/nature-spiritsmp3-1/ |
| 121 | Almusic34 | Sequential Soundscape | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/sequential-soundscapemp3/ |
| 122 | Almusic34 | Presence in the Night | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/presence-in-the-nightmp3-1/ |
| 123 | Almusic34 | Journey in the Wind | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/journey-in-the-windmp3-1/ |
| 124 | Almusic34 | Mysterious Landscape | CC-BY 4.0 | https://freemusicarchive.org/music/almusic34/single/mysterious-landscapemp3/ |
| 125 | Splashkabona | Inspiring Positive Cinematic Calming Ambient Piano | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/inspiring-positive-cinematic-calming-ambient-piano/ |
| 126 | Splashkabona | Meditative Zen Yoga Spa Chill Out Ambient | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/meditative-zen-yoga-spa-chill-out-ambient/ |
| 127 | Splashkabona | Smooth Inspiring Background | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/smooth-inspiring-background/ |
| 128 | Splashkabona | Chill Ambient Elegant Pop Background | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/chill-ambient-elegant-pop-background/ |
| 129 | Splashkabona | Dark Cinematic Ambient | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/dark-cinematic-ambient/ |
| 130 | Splashkabona | Deep Chill Electronic | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/deep-chill-electronic/ |
| 131 | Splashkabona | Prosperous Downtempo Chillwave Ambient | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/prosperous-downtempo-chillwave-ambient/ |
| 132 | Splashkabona | Ethereal Veil | CC-BY 4.0 | https://freemusicarchive.org/music/splashkabona/single/ethereal-veil/ |
| 133 | 1000 Handz | Opportunity | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/opportunity/ |
| 134 | 1000 Handz | Embers | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/embers-2/ |
| 135 | 1000 Handz | Flowers | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/flowers-3/ |
| 136 | 1000 Handz | Lovely | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/lovely-1/ |
| 137 | 1000 Handz | Leverage | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/leverage/ |
| 138 | 1000 Handz | Seasons | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/seasons/ |
| 139 | 1000 Handz | Early | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/early/ |
| 140 | 1000 Handz | Branches | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/branches-1/ |
| 141 | 1000 Handz | Tales | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/tales/ |
| 142 | 1000 Handz | Spring | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/spring-5/ |
| 143 | 1000 Handz | Void | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/void-3/ |
| 144 | 1000 Handz | Growth | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-solo-piano-melodies/growth-2/ |
| 145 | 1000 Handz | Velvet ft. Ketsa | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-chillstudylounge-instrumentals/velvet-ft-ketsa/ |
| 146 | 1000 Handz | Pay It Forward | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-chillstudylounge-instrumentals/pay-it-forward/ |
| 147 | 1000 Handz | Neon | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-chillstudylounge-instrumentals/neon-1/ |
| 148 | 1000 Handz | Chill Out | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-chillstudylounge-instrumentals/chill-out-1/ |
| 149 | 1000 Handz | Water Cooler | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-corporatead-instrumentals/water-cooler/ |
| 150 | 1000 Handz | Casual Fridays | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-corporatead-instrumentals/casual-fridays/ |
| 151 | 1000 Handz | Clock Out | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-corporatead-instrumentals/clock-out/ |
| 152 | 1000 Handz | Lunch Break | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-corporatead-instrumentals/lunch-break/ |
| 153 | 1000 Handz | Office Plants | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-corporatead-instrumentals/office-plants/ |
| 154 | 1000 Handz | Sunset Love ft. Ketsa | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-tropical-vibes/sunset-love-ft-ketsa/ |
| 155 | 1000 Handz | Cocoon | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-tropical-vibes/cocoon-1/ |
| 156 | 1000 Handz | Turquoise Water | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-tropical-vibes/turqouise-water/ |
| 157 | 1000 Handz | Bloom | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-tropical-vibes/bloom-3/ |
| 158 | 1000 Handz | Agua | CC-BY 4.0 | https://freemusicarchive.org/music/1000-handz/cc-by-free-to-use-tropical-vibes/agua-1/ |
| 159 | Lobo Loco | Christmas Market | CC-BY 4.0 | https://freemusicarchive.org/music/Lobo_Loco/christmas-market-cc-by/christmas-market-id-2402/ |
| 160 | Lobo Loco | Land of Silence | CC-BY 4.0 | https://freemusicarchive.org/music/Lobo_Loco/christmas-market-cc-by/land-of-silence-id-2399/ |
| 161 | Lobo Loco | Lama Shadow | CC-BY 4.0 | https://freemusicarchive.org/music/Lobo_Loco/christmas-market-cc-by/lama-shadow-id-2400-1/ |
| 162 | Lobo Loco | Sheperd Angels | CC-BY 4.0 | https://freemusicarchive.org/music/Lobo_Loco/christmas-market-cc-by/sheperd-angels-id-2405/ |
| 163 | Lobo Loco | Beach and Surf | CC-BY 4.0 | https://freemusicarchive.org/music/Lobo_Loco/free-for-you-cc-by/beach-and-surf-id-2361/ |
| 164 | Lobo Loco | Celltrance | CC-BY 4.0 | https://freemusicarchive.org/music/Lobo_Loco/free-for-you-cc-by/celltrance-id-2346/ |
| 165 | Sergey Cheremisinov | Limitless | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/slow-light/limitless-1/ |
| 166 | Sergey Cheremisinov | Slow Light | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/slow-light/slow-light/ |
| 167 | Sergey Cheremisinov | Sense | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/slow-light/sense-1/ |
| 168 | Andy G. Cohen | Piscoid | CC-BY 4.0 | https://freemusicarchive.org/music/Andy_G_Cohen/MUL__DIV_1198/Andy_G_Cohen_-_MULDIV_-_01_-_Piscoid_1803/ |
| 169 | Andy G. Cohen | Land Legs | CC-BY 4.0 | https://freemusicarchive.org/music/Andy_G_Cohen/MUL__DIV_1198/Andy_G_Cohen_-_MULDIV_-_02_-_Land_Legs/ |
| 170 | Andy G. Cohen | Oxygen Mask | CC-BY 4.0 | https://freemusicarchive.org/music/Andy_G_Cohen/MUL__DIV_1198/Andy_G_Cohen_-_MULDIV_-_03_-_Oxygen_Mask/ |
| 171 | Andy G. Cohen | A Perceptible Shift | CC-BY 4.0 | https://freemusicarchive.org/music/Andy_G_Cohen/MUL__DIV_1198/Andy_G_Cohen_-_MULDIV_-_04_-_A_Perceptible_Shift/ |
| 172 | Andy G. Cohen | Bathed in Fine Dust | CC-BY 4.0 | https://freemusicarchive.org/music/Andy_G_Cohen/MUL__DIV_1198/Andy_G_Cohen_-_MULDIV_-_07_-_Bathed_in_Fine_Dust/ |
| 173 | Andy G. Cohen | Warmer | CC-BY 4.0 | https://freemusicarchive.org/music/Andy_G_Cohen/MUL__DIV_1198/Andy_G_Cohen_-_MULDIV_-_10_-_Warmer/ |
| 174 | Komiku | The road we use to travel when we were kids | CC0 | https://freemusicarchive.org/music/Komiku/Tale_on_the_Late/Komiku_-_Tale_on_the_Late_-_03_The_road_we_use_to_travel_when_we_were_kids/ |
| 175 | Komiku | Village, 2068 | CC0 | https://freemusicarchive.org/music/Komiku/Tale_on_the_Late/Komiku_-_Tale_on_the_Late_-_09_Village_2068/ |
| 176 | Komiku | You can't beat the machine | CC0 | https://freemusicarchive.org/music/Komiku/Tale_on_the_Late/Komiku_-_Tale_on_the_Late_-_15_You_cant_beat_the_machine/ |
| 177 | Komiku | End of the trip | CC0 | https://freemusicarchive.org/music/Komiku/Tale_on_the_Late/Komiku_-_Tale_on_the_Late_-_16_End_of_the_trip/ |
| 178 | The Imperfectionist | Free space ambient music 1 - Take off | CC-BY 4.0 | https://freemusicarchive.org/music/the-imperfectionist/single/free-space-ambient-music-1-take-off/ |
| 179 | The Imperfectionist | Free ambient music 1 - Windy mountains | CC-BY 4.0 | https://freemusicarchive.org/music/the-imperfectionist/single/free-ambient-music-1-windy-mountainsmp3/ |
| 180 | Sergey Cheremisinov | Closer To You | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_01_Closer_To_You/ |
| 181 | Sergey Cheremisinov | Train | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_02_Train/ |
| 182 | Sergey Cheremisinov | Waves | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_03_Waves/ |
| 183 | Sergey Cheremisinov | When You Leave | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_04_When_You_Leave/ |
| 184 | Sergey Cheremisinov | Fog | CC-BY 4.0 | https://freemusicarchive.org/music/Sergey_Cheremisinov/Charms/Sergey_Cheremisinov_-_Charms_-_05_Fog/ |
| 185 | Komiku | Fouler l'horizon | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_01_Fouler_lhorizon/ |
| 186 | Komiku | Le Grand Village | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_02_Le_Grand_Village/ |
| 187 | Komiku | Champ de tournesol | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_03_Champ_de_tournesol/ |
| 188 | Komiku | Barque sur le lac | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_04_Barque_sur_le_lac/ |
| 189 | Komiku | De l'herbe sous les pieds | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_09_De_lherbe_sous_les_pieds/ |
| 190 | Komiku | Bleu | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_13_Bleu/ |
| 191 | Komiku | Un coin loin du monde | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure_/Komiku_-_Its_time_for_adventure_-_14_Un_coin_loin_du_monde/ |
| 192 | Komiku | Balance | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_01_Balance/ |
| 193 | Komiku | Chill Out Theme | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_02_Chill_Out_Theme/ |
| 194 | Komiku | Time | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_04_Time/ |
| 195 | Komiku | Down the river | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_05_Down_the_river/ |
| 196 | Komiku | Frozen Jungle | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_07_Frozen_Jungle/ |
| 197 | Komiku | Dreaming of you | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_2/Komiku_-_Its_time_for_adventure_vol_2_-_08_Dreaming_of_you/ |
| 198 | Komiku | Childhood scene | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_3/Komiku_-_Its_time_for_adventure_vol_3_-_01_Childhood_scene/ |
| 199 | Komiku | The place that never gets old | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_3/Komiku_-_Its_time_for_adventure_vol_3_-_07_The_place_that_never_get_old/ |
| 200 | Komiku | Xenobiological Forest | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_5/Komiku_-_Its_time_for_adventure_vol_5_-_05_Xenobiological_Forest/ |
| 201 | Komiku | Friends's theme | CC0 | https://freemusicarchive.org/music/Komiku/Its_time_for_adventure__vol_5/Komiku_-_Its_time_for_adventure_vol_5_-_06_Friendss_theme/ |
| 202 | HoliznaCC0 | Lullabies For The End Of The World 1 | CC0 | https://freemusicarchive.org/music/holiznacc0/lullabies-for-the-end-of-the-world/lullabies-for-the-end-of-the-world-1/ |
| 203 | HoliznaCC0 | Lullabies For The End Of The World 2 | CC0 | https://freemusicarchive.org/music/holiznacc0/lullabies-for-the-end-of-the-world/lullabies-for-the-end-of-the-world-2/ |
| 204 | HoliznaCC0 | Lullabies For The End Of The World 3 | CC0 | https://freemusicarchive.org/music/holiznacc0/lullabies-for-the-end-of-the-world/lullabies-for-the-end-of-the-world-3/ |

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
