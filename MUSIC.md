# Music Inventory

This file tracks the local music catalog used by `late.sh` radio.

- Runtime source of truth for playback order is the `.m3u` files in `infra/liquidsoap/`.
- Source of truth for reproducible fetching is `scripts/fetch_cc_music.py` plus `scripts/fetch_ambient_refresh.py` for the expanded ambient catalog.
- `CONTEXT.md` should keep only high-signal status and point here for detailed track inventories.

## Library Status

- `lofi`: done, 202-track manifest, mixed `CC0` and `CC-BY 4.0`
- `ambient`: done, 204 tracks, mixed `CC0` and `CC-BY 4.0`
- `classic`: done, 201-track calm-first manifest, public domain via Musopen / Internet Archive (Musopen Chopin CC0 + Kimiko Ishizaka's "Open Well-Tempered Clavier" CC0/PD-mark + MusopenCollectionAsFlac PD)
- `jazz`: pending

## Lofi

This section documents the current 202-track lofi manifest used by the regenerated playlist files. The dev Liquidsoap stack now mounts `tmp/music/lofi` onto `/music/lofi`, so the local runtime playlist resolves against the refreshed temp library.

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
| 25 | HoliznaCC0 | Bubbles | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 26 | HoliznaCC0 | Calm Current | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 27 | HoliznaCC0 | Color Of A Soul | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 28 | HoliznaCC0 | Complicated Feelings | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 29 | HoliznaCC0 | Dream shifter | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 30 | HoliznaCC0 | Dreamy Reverie | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 31 | HoliznaCC0 | Ease Into Night | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 32 | HoliznaCC0 | Infinite Echoes | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 33 | HoliznaCC0 | Into The Mist | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 34 | HoliznaCC0 | Lucid | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 35 | HoliznaCC0 | Never Sleeping | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 36 | HoliznaCC0 | Ode To Forgetting | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 37 | HoliznaCC0 | Peaceful Drift | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 38 | HoliznaCC0 | Reminders | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 39 | HoliznaCC0 | Saturation | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 40 | HoliznaCC0 | Walking Away | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 41 | HoliznaCC0 | Wave Maker | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 42 | HoliznaCC0 | Wetlands | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 43 | HoliznaCC0 | Canon Event | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 44 | HoliznaCC0 | Moon Unit | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 45 | HoliznaCC0 | One Night In France | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 46 | HoliznaCC0 | Still Life | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 47 | HoliznaCC0 | Theta Frequency | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 48 | HoliznaCC0 | Tokyo Sunset | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 49 | HoliznaCC0 | Tranquil Mindset | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 50 | HoliznaCC0 | Blue Skies | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 51 | HoliznaCC0 | laundry On The Wire | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 52 | HoliznaCC0 | Waves | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 53 | HoliznaCC0 | Windows Down | CC0 | https://holiznacc0.bandcamp.com/album/public-domain-lo-fi |
| 54 | HoliznaCC0 | First Snow | CC0 | https://holiznacc0.bandcamp.com/album/winter-lo-fi-2 |
| 55 | HoliznaCC0 | Snow Drift | CC0 | https://holiznacc0.bandcamp.com/album/winter-lo-fi-2 |
| 56 | HoliznaCC0 | 2 Hour Delay | CC0 | https://holiznacc0.bandcamp.com/album/winter-lo-fi-2 |
| 57 | HoliznaCC0 | Fire Place | CC0 | https://holiznacc0.bandcamp.com/album/winter-lo-fi-2 |
| 58 | HoliznaCC0 | Winter Blues | CC0 | https://holiznacc0.bandcamp.com/album/winter-lo-fi-2 |
| 59 | HoliznaCC0 | Busking In The SunLight | CC0 | https://holiznacc0.bandcamp.com/album/city-slacker |
| 60 | HoliznaCC0 | Bus Stop | CC0 | https://holiznacc0.bandcamp.com/album/city-slacker |
| 61 | HoliznaCC0 | Busted Ac Unit | CC0 | https://holiznacc0.bandcamp.com/album/city-slacker |
| 62 | HoliznaCC0 | Nowhere To Be, Nothing To Do | CC0 | https://holiznacc0.bandcamp.com/album/city-slacker |
| 63 | Ketsa | Tetra | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/tetra/ |
| 64 | Ketsa | I Dream Of You | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/i-dream-of-you/ |
| 65 | Ketsa | Black Screen | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/black-screen/ |
| 66 | Ketsa | Slow Dance | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/slow-dance/ |
| 67 | Ketsa | Seconds Left | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/seconds-left/ |
| 68 | Ketsa | Lowest Sun | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/lowest-sun/ |
| 69 | Ketsa | Down Pitch | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/down-pitch/ |
| 70 | Ketsa | Reclaimed | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/reclaimed/ |
| 71 | Ketsa | The Time It Takes | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/the-time-it-takes/ |
| 72 | Ketsa | Deep Waves | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/deep-waves/ |
| 73 | Ketsa | Shining Still | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/shining-still/ |
| 74 | Ketsa | The Winter Months | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/the-winter-months/ |
| 75 | Ketsa | Folded | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/lofi-downtempo/folded/ |
| 76 | Ketsa | Home Sigh | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/home-sigh/ |
| 77 | Ketsa | Take Me Up | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/take-me-up/ |
| 78 | Ketsa | Appointments | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/appointments/ |
| 79 | Ketsa | Jazz Daze | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/jazz-daze/ |
| 80 | Ketsa | Bring Dat | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/bring-dat/ |
| 81 | Ketsa | Make Me Sad | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/make-me-sad/ |
| 82 | Ketsa | In Trouble | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/in-trouble/ |
| 83 | Ketsa | World's A Stage | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/worlds-a-stage/ |
| 84 | Ketsa | Smoothness | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/smoothness/ |
| 85 | Ketsa | Journal | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/journal/ |
| 86 | Ketsa | My Biz | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/my-biz/ |
| 87 | Ketsa | Aligning Frequencies | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/aligning-frequencies/ |
| 88 | Ketsa | Therapy | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/therapy/ |
| 89 | Ketsa | Sun Slides | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/sun-slides/ |
| 90 | Ketsa | To do | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/to-do/ |
| 91 | Ketsa | Grand Rising | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/grand-rising/ |
| 92 | Ketsa | The Cure | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/the-cure/ |
| 93 | Ketsa | Keep Hold | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/vintage-beats/keep-hold/ |
| 94 | JMHBM | One More | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/one-more/ |
| 95 | JMHBM | Night City | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/night-city/ |
| 96 | JMHBM | New New | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/new-new/ |
| 97 | JMHBM | Do Me Right | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/do-me-right/ |
| 98 | JMHBM | Heavyweights | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/heavyweights/ |
| 99 | JMHBM | Footsteps | CC-BY 4.0 | https://freemusicarchive.org/music/beat-mekanik/single/footsteps/ |
| 100 | legacyAlli | RF - LoFi Funky and Chunky | CC-BY 4.0 | https://freemusicarchive.org/music/legacyalli/instrumental-by-legacyalli-2024/rf-lofi-funky-and-chunky/ |
| 101 | HoliznaCC0 | Anxiety | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 102 | HoliznaCC0 | Boredom | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 103 | HoliznaCC0 | Deja Vu | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 104 | HoliznaCC0 | Love | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 105 | HoliznaCC0 | Memories | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 106 | HoliznaCC0 | Childhood | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 107 | HoliznaCC0 | Dancing | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 108 | HoliznaCC0 | Day Jobs | CC0 | https://holiznacc0.bandcamp.com/album/only-in-the-milky-way |
| 109 | HoliznaCC0 | City In The Rearview | CC0 | https://holiznacc0.bandcamp.com/album/we-drove-all-night |
| 110 | HoliznaCC0 | I Thought You Were Cool | CC0 | https://holiznacc0.bandcamp.com/album/we-drove-all-night |
| 111 | HoliznaCC0 | Quiet Moonlit Countrysides | CC0 | https://holiznacc0.bandcamp.com/album/we-drove-all-night |
| 112 | HoliznaCC0 | Stealing Glimpses Of Your Face | CC0 | https://holiznacc0.bandcamp.com/album/we-drove-all-night |
| 113 | HoliznaCC0 | Morning Light | CC0 | https://holiznacc0.bandcamp.com/album/we-drove-all-night |
| 114 | HoliznaCC0 | Eat | CC0 | https://holiznacc0.bandcamp.com/album/bassic |
| 115 | HoliznaCC0 | Sleep | CC0 | https://holiznacc0.bandcamp.com/album/bassic |
| 116 | HoliznaCC0 | Breath | CC0 | https://holiznacc0.bandcamp.com/album/bassic |
| 117 | HoliznaCC0 | Make Money | CC0 | https://holiznacc0.bandcamp.com/album/bassic |
| 118 | HoliznaCC0 | Make Love | CC0 | https://holiznacc0.bandcamp.com/album/bassic |
| 119 | HoliznaCC0 | Final Level | CC0 | https://holiznacc0.bandcamp.com/album/gamer-beats |
| 120 | HoliznaCC0 | Coins | CC0 | https://holiznacc0.bandcamp.com/album/gamer-beats |
| 121 | HoliznaCC0 | Legends | CC0 | https://holiznacc0.bandcamp.com/album/gamer-beats |
| 122 | HoliznaCC0 | A Fight In The Dark | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 123 | HoliznaCC0 | A Hero Is Born | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 124 | HoliznaCC0 | All The Fight Left | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 125 | HoliznaCC0 | Bag Of Carrying | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 126 | HoliznaCC0 | City Limits | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 127 | HoliznaCC0 | Comfort Game #1 | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 128 | HoliznaCC0 | Comfort Game #2 | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 129 | HoliznaCC0 | Comfort Game #3 | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 130 | HoliznaCC0 | Comfort Game #4 | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 131 | HoliznaCC0 | Credits | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 132 | HoliznaCC0 | Cyber Anxiety | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 133 | HoliznaCC0 | Fires Uptown | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 134 | HoliznaCC0 | Flying | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 135 | HoliznaCC0 | Half Machine | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 136 | HoliznaCC0 | Home | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 137 | HoliznaCC0 | Internal Panic | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 138 | HoliznaCC0 | Jump | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 139 | HoliznaCC0 | Machines With Feelings | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 140 | HoliznaCC0 | Magic Orb | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 141 | HoliznaCC0 | Mini Boss | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 142 | HoliznaCC0 | Mystery | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 143 | HoliznaCC0 | New Gods | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 144 | HoliznaCC0 | Night Life | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 145 | HoliznaCC0 | Quickly! | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 146 | HoliznaCC0 | Random Encounter | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 147 | HoliznaCC0 | Righteous Sword | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 148 | HoliznaCC0 | Secret Map | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 149 | HoliznaCC0 | Skyline | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 150 | HoliznaCC0 | Street Lights Passing By | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 151 | HoliznaCC0 | Trees In The Fog | CC0 | https://holiznacc0.bandcamp.com/album/eternal-skies-retro-gamer |
| 152 | HoliznaCC0 | We Used To Dance | CC0 | https://holiznacc0.bandcamp.com/album/waves-of-nostalgia-part-2 |
| 153 | HoliznaCC0 | A Small Town On Pluto (Composed) | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/a-small-town-on-pluto-composed/ |
| 154 | HoliznaCC0 | A Small Town On Pluto (Music Box) | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/a-small-town-on-pluto-music-box/ |
| 155 | HoliznaCC0 | Cabin Fever | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/cabin-fever/ |
| 156 | HoliznaCC0 | Creepy Piano 1 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-1/ |
| 157 | HoliznaCC0 | Creepy Piano 2 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-2/ |
| 158 | HoliznaCC0 | Creepy Piano 3 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-3/ |
| 159 | HoliznaCC0 | Creepy Piano 4 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/creepy-piano-4/ |
| 160 | HoliznaCC0 | Dangerous Voyage (Music Box) | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/dangerous-voyage-music-box/ |
| 161 | HoliznaCC0 | Dangerous Voyage | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/dangerous-voyage/ |
| 162 | HoliznaCC0 | Drifting Piano | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/drifting-piano/ |
| 163 | HoliznaCC0 | Game Travel 1 (Piano) | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/game-travel-1-piano/ |
| 164 | HoliznaCC0 | OST Music Box 1 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-1/ |
| 165 | HoliznaCC0 | OST Music Box 2 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-2/ |
| 166 | HoliznaCC0 | OST Music Box 3 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-3/ |
| 167 | HoliznaCC0 | OST Music Box 4 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-4/ |
| 168 | HoliznaCC0 | OST Music Box 5 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-5/ |
| 169 | HoliznaCC0 | OST Music Box 6 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-6/ |
| 170 | HoliznaCC0 | OST Music Box 7 | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/ost-music-box-7/ |
| 171 | HoliznaCC0 | Spring On The Horizon | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/spring-on-the-horizon/ |
| 172 | HoliznaCC0 | VST Guitar | CC0 | https://freemusicarchive.org/music/holiznacc0/background-music/vst-guitar/ |
| 173 | Ketsa | A Little Faith | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/a-little-faith/ |
| 174 | Ketsa | All Ways | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/all-ways/ |
| 175 | Ketsa | Always Faithful | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/always-faithful/ |
| 176 | Ketsa | Brazilian Sunsets | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/brazilian-sunsets/ |
| 177 | Ketsa | Bright State | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/bright-state/ |
| 178 | Ketsa | Cello | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/cello/ |
| 179 | Ketsa | Dawn Faded | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/dawn-faded/ |
| 180 | Ketsa | Dry and High | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/dry-and-high/ |
| 181 | Ketsa | Feeling | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/feeling-1/ |
| 182 | Ketsa | Good Feel | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/good-feel/ |
| 183 | Ketsa | Her Memory Fading | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/her-memory-fading/ |
| 184 | Ketsa | Here For You | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/here-for-you/ |
| 185 | Ketsa | Importance | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/importance-1/ |
| 186 | Ketsa | Inside Dead | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/inside-dead/ |
| 187 | Ketsa | Kinship | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/kinship/ |
| 188 | Ketsa | Life is Great | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/life-is-great/ |
| 189 | Ketsa | London West | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/london-west/ |
| 190 | Ketsa | Longer Wait | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/longer-wait/ |
| 191 | Ketsa | Night Flow Day Grow | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/night-flow-day-grow/ |
| 192 | Ketsa | Off Days | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/off-days/ |
| 193 | Ketsa | Saviour Above | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/saviour-above/ |
| 194 | Ketsa | That Feeling | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/that-feeling/ |
| 195 | Ketsa | The Road 2 | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/the-road-2/ |
| 196 | Ketsa | The Road | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/the-road-1/ |
| 197 | Ketsa | Tide Turns | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/tide-turns/ |
| 198 | Ketsa | Too Late | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/too-late/ |
| 199 | Ketsa | Trench Work | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/trench-work/ |
| 200 | Ketsa | Vision | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/vision-2/ |
| 201 | Ketsa | What It Feels Like | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/what-it-feels-like-1/ |
| 202 | Ketsa | Will Make You Happy | CC-BY 4.0 | https://freemusicarchive.org/music/Ketsa/cc-by-free-to-use-for-anything/will-make-you-happy/ |

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

This section documents the current 201-track calm-first classical manifest used by the regenerated playlist files. The dev Liquidsoap stack mounts `tmp/music/classic` onto `/music/classic`, so the local runtime playlist resolves against the refreshed temp library. Tracks 1-100 are the original Musopen Public Domain pass; tracks 101-201 are the second-pass coding-session expansion (5 chill leftovers from MusopenCollectionAsFlac, 48 Bach Well-Tempered Clavier Book I prelude/fugue pairs by Kimiko Ishizaka under CC0/PD mark, and 48 Chopin pieces from Musopen's CC0 Complete Chopin Collection).

| # | Artist | Title | License | Source URL |
|---|--------|-------|---------|------------|
| 1 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Aria | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-01-GoldbergVariationsBwv.988-Aria.mp3 |
| 2 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 1 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-02-GoldbergVariationsBwv.988-Variation1.mp3 |
| 3 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 2 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-03-GoldbergVariationsBwv.988-Variation2.mp3 |
| 4 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 3. Canon on the unison | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-04-GoldbergVariationsBwv.988-Variation3.CanonOnTheUnison.mp3 |
| 5 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 4 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-05-GoldbergVariationsBwv.988-Variation4.mp3 |
| 6 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 5 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-06-GoldbergVariationsBwv.988-Variation5.mp3 |
| 7 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 6. Canon on the second | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-07-GoldbergVariationsBwv.988-Variation6.CanonOnTheSecond.mp3 |
| 8 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 7 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-08-GoldbergVariationsBwv.988-Variation7.mp3 |
| 9 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 8 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-09-GoldbergVariationsBwv.988-Variation8.mp3 |
| 10 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 9. Canon on the third | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-10-GoldbergVariationsBwv.988-Variation9.CanonOnTheThird.mp3 |
| 11 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 10. Fughetta | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-11-GoldbergVariationsBwv.988-Variation10.Fughetta.mp3 |
| 12 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 11 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-12-GoldbergVariationsBwv.988-Variation11.mp3 |
| 13 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 12. Canon on the fourth | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-13-GoldbergVariationsBwv.988-Variation12.CanonOnTheFourth.mp3 |
| 14 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 13 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-14-GoldbergVariationsBwv.988-Variation13.mp3 |
| 15 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 14 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-15-GoldbergVariationsBwv.988-Variation14.mp3 |
| 16 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 15. Canon on the fifth | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-16-GoldbergVariationsBwv.988-Variation15.CanonOnTheFifth.mp3 |
| 17 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 16. Overture | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-17-GoldbergVariationsBwv.988-Variation16.Overture.mp3 |
| 18 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 17 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-18-GoldbergVariationsBwv.988-Variation17.mp3 |
| 19 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 18. Canon on the sixth | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-19-GoldbergVariationsBwv.988-Variation18.CanonOnTheSixth.mp3 |
| 20 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 19 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-20-GoldbergVariationsBwv.988-Variation19.mp3 |
| 21 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 20 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-21-GoldbergVariationsBwv.988-Variation20.mp3 |
| 22 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 21. Canon on the seventh | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-22-GoldbergVariationsBwv.988-Variation21.CanonOnTheSeventh.mp3 |
| 23 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 22 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-23-GoldbergVariationsBwv.988-Variation22.mp3 |
| 24 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 23 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-24-GoldbergVariationsBwv.988-Variation23.mp3 |
| 25 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 24. Canon on the octave | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-25-GoldbergVariationsBwv.988-Variation24.CanonOnTheOctave.mp3 |
| 26 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 25 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-26-GoldbergVariationsBwv.988-Variation25.mp3 |
| 27 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 26 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-27-GoldbergVariationsBwv.988-Variation26.mp3 |
| 28 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 27. Canon on the ninth | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-28-GoldbergVariationsBwv.988-Variation27.CanonOnTheNinth.mp3 |
| 29 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 28 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-29-GoldbergVariationsBwv.988-Variation28.mp3 |
| 30 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 29 | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-30-GoldbergVariationsBwv.988-Variation29.mp3 |
| 31 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Variation 30. Quodlibet | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-31-GoldbergVariationsBwv.988-Variation30.Quodlibet.mp3 |
| 32 | Johann Sebastian Bach | Goldberg Variations, BWV 988 - Aria Da Capo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Bach_GoldbergVariations/JohannSebastianBach-32-GoldbergVariationsBwv.988-AriaDaCapo.mp3 |
| 33 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - I. Allegro con brio | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-01-AllegroConBrio.mp3 |
| 34 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - II. Adagio ma non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-02-AdagioMaNonTroppo.mp3 |
| 35 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - III. Scherzo Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-03-ScherzoAllegro.mp3 |
| 36 | Ludwig van Beethoven | String Quartet No. 6 in B-flat Major, Op. 18 No. 6 - IV. La Malinconia | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Beethoven_StringQuartetNo.6inBFlatMajorOp.18/LudwigVanBeethoven-StringQuartetNo.6InBFlatMajorOp.18No.6-04-adagioLaMalinconia.mp3 |
| 37 | Wolfgang Amadeus Mozart | String Quartet No. 15 in D Minor, K. 421 - I. Allegro moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-01-AllegroModerato.mp3 |
| 38 | Ludwig van Beethoven | Symphony No. 3 in E Flat Major Eroica, Op. 55 - 02 - Marcia funebre Adagio assai | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Beethoven_SymphonyNo.3Eroica/LudwigVanBeethoven-SymphonyNo.3InEFlatMajorEroicaOp.55-02-MarciaFunebreAdagioAssai.mp3 |
| 39 | Wolfgang Amadeus Mozart | String Quartet No. 15 in D Minor, K. 421 - II. Andante | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-02-Andante.mp3 |
| 40 | Wolfgang Amadeus Mozart | String Quartet No. 15 in D Minor, K. 421 - III. Minuetto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-03-Minuetto.mp3 |
| 41 | Alexander Borodin | String Quartet No. 1 in A Major - 01 - Moderato - Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.1inAMajor/AlexanderBorodin-StringQuartetNo.1InAMajor-01-Moderato-Allegro.mp3 |
| 42 | Alexander Borodin | String Quartet No. 1 in A Major - 02 - Andante con moto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.1inAMajor/AlexanderBorodin-StringQuartetNo.1InAMajor-02-AndanteConMoto.mp3 |
| 43 | Wolfgang Amadeus Mozart | String Quartet No. 15 in D Minor, K. 421 - IV. Allegro ma non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.15inDMinorK421/WolfgangAmadeusMozart-StringQuartetNo.15InDMinorK421-04-AllegroMaNonTroppo.mp3 |
| 44 | Alexander Borodin | String Quartet No. 1 in A Major - 04 - Andante - Allegro risoluto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.1inAMajor/AlexanderBorodin-StringQuartetNo.1InAMajor-04-Andante-AllegroRisoluto.mp3 |
| 45 | Alexander Borodin | String Quartet No. 2 in D Major - I. Allegro moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-01-AllegroModerato.mp3 |
| 46 | Alexander Borodin | String Quartet No. 2 in D Major - II. Scherzo Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-02-ScherzoAllegro.mp3 |
| 47 | Alexander Borodin | String Quartet No. 2 in D Major - III. Nocturne Andante | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-03-NocturneAndante.mp3 |
| 48 | Alexander Borodin | String Quartet No. 2 in D Major - IV. Finale Andante - Vivace | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_StringQuartetNo.2inDMajor/AlexanderBorodin-StringQuartetNo.2InDMajor-04-FinaleAndante-Vivace.mp3 |
| 49 | Franz Schubert | Sonata in A Minor, D. 845 - I. Moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMinorD.845/FranzSchubert-SonataInAMinorD.845-01-Moderato.mp3 |
| 50 | Johannes Brahms | Symphony No. 1 in C Minor, Op. 68 - 02 - Andante sostenuto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.1inCMinor/JohannesBrahms-SymphonyNo.1InCMinorOp.68-02-AndanteSostenuto.mp3 |
| 51 | Johannes Brahms | Symphony No. 1 in C Minor, Op. 68 - 03 - Un poco allegretto e grazioso | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.1inCMinor/JohannesBrahms-SymphonyNo.1InCMinorOp.68-03-UnPocoAllegrettoEGrazioso.mp3 |
| 52 | Franz Schubert | Sonata in A Minor, D. 845 - II. Andante poco mosso | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMinorD.845/FranzSchubert-SonataInAMinorD.845-02-AndantePocoMosso.mp3 |
| 53 | Johannes Brahms | Symphony No. 3 in F Major, Op. 90 - 01 - Allegro con brio | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-01-AllegroConBrio.mp3 |
| 54 | Johannes Brahms | Symphony No. 3 in F Major, Op. 90 - 02 - Andante | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-02-Andante.mp3 |
| 55 | Johannes Brahms | Symphony No. 3 in F Major, Op. 90 - 03 - Poco allegretto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-03-PocoAllegretto.mp3 |
| 56 | Johannes Brahms | Symphony No. 3 in F Major, Op. 90 - 04 - Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.3inFMajor/JohannesBrahms-SymphonyNo.3InFMajorOp.90-04-Allegro.mp3 |
| 57 | Johannes Brahms | Symphony No. 4 in E Minor, Op. 98 - 01 - Allegro Non Troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.4inEMinor/JohannesBrahms-SymphonyNo.4InEMinorOp.98-01-AllegroNonTroppo.mp3 |
| 58 | Johannes Brahms | Symphony No. 4 in E Minor, Op. 98 - 02 - Andante Moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.4inEMinor/JohannesBrahms-SymphonyNo.4InEMinorOp.98-02-AndanteModerato.mp3 |
| 59 | Franz Schubert | Sonata in A Minor, D. 959 - II. Andantino | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMinorD.959/FranzSchubert-SonataInAMinorD.959-02-Andantino.mp3 |
| 60 | Franz Schubert | Sonata in A Minor, D. 959 - IV. Rondo Allegretto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMinorD.959/FranzSchubert-SonataInAMinorD.959-04-Rondo.Allegretto.mp3 |
| 61 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - I. Allegro ma non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-01-AllegroMaNonTroppo.mp3 |
| 62 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - II. Lento | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-02-Lento.mp3 |
| 63 | Franz Schubert | Sonata in C Minor, D. 958 - II. Adagio | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInCMinorD.958/FranzSchubert-SonataInCMinorD.958-02-Adagio.mp3 |
| 64 | Antonin Dvorak | String Quartet No. 12 in F Major, Op. 96 'American' - IV. Finale Vivace ma non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Dvorak_StringQuartetNo.12inFMajorOp.96/AntonnDvorak-StringQuartetNo.12InFMajorOp.96American-04-Finale-VivaceMaNonTroppo.mp3 |
| 65 | Antonin Dvorak | String Quartet No. 10 in E Flat, Op. 51 - 01 - Allegro Ma Non Troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Dvorak_StringQuartetNo.10inEFlatOp.51/AntonnDvorak-StringQuartetNo.10InEFlatOp.51-01-AllegroMaNonTroppo.mp3 |
| 66 | Antonin Dvorak | String Quartet No. 10 in E Flat, Op. 51 - 02 - Dumka | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Dvorak_StringQuartetNo.10inEFlatOp.51/AntonnDvorak-StringQuartetNo.10InEFlatOp.51-02-Dumka.mp3 |
| 67 | Antonin Dvorak | String Quartet No. 10 in E Flat, Op. 51 - 03 - Romanza | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Dvorak_StringQuartetNo.10inEFlatOp.51/AntonnDvorak-StringQuartetNo.10InEFlatOp.51-03-Romanza.mp3 |
| 68 | Franz Schubert | Sonata in A Minor, D. 784 - II. Andante | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMinorD.784/FranzSchubert-SonataInAMinorD.784-02-Andante.mp3 |
| 69 | Edvard Grieg | Peer Gynt Suite No. 1, Op. 46 - 01 - Morning | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-01-Morning.mp3 |
| 70 | Edvard Grieg | Peer Gynt Suite No. 1, Op. 46 - 02 - Aase's Death | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-02-AasesDeath.mp3 |
| 71 | Edvard Grieg | Peer Gynt Suite No. 1, Op. 46 - 03 - Anitra's Dream | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-03-AnitrasDream.mp3 |
| 72 | Felix Mendelssohn | Symphony No. 3 in A Minor 'Scottish', Op. 56 - I. Andante con moto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_ScottishSymphony/FelixMendelssohn-SymphonyNo.3InAMinorscottishOp.56-01-AndanteConMoto.mp3 |
| 73 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - I. Allegro moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-01-AllegroModerato.mp3 |
| 74 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - II. Adagio cantabile | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-02-AdagioCantabile.mp3 |
| 75 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - III. Menuetto Allegretto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-03-MenuettoAllegretto.mp3 |
| 76 | Joseph Haydn | String Quartet in D Major, Op. 64 No. 5 'Lark' - IV. Finale Vivace | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Haydn_StringQuartetInDMajorOp.64/JosephHaydn-StringQuartetInDOp.645H363Lark-04-FinaleVivace.mp3 |
| 77 | Felix Mendelssohn | Symphony No. 4 in A Major, Op. 90 'Italian' - 01 - Allegro vivace | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_ItalianSymphony/FelixMendelssohn-SymphonyNo.4InAMajorOp.90italian-01-AllegroVivace.mp3 |
| 78 | Felix Mendelssohn | Symphony No. 4 in A Major, Op. 90 'Italian' - 02 - Andante con moto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_ItalianSymphony/FelixMendelssohn-SymphonyNo.4InAMajorOp.90italian-02-AndanteConMoto.mp3 |
| 79 | Felix Mendelssohn | Symphony No. 4 in A Major, Op. 90 'Italian' - 03 - Con moto moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_ItalianSymphony/FelixMendelssohn-SymphonyNo.4InAMajorOp.90italian-03-ConMotoModerato.mp3 |
| 80 | Felix Mendelssohn | Symphony No. 3 in A Minor 'Scottish', Op. 56 - III. Adagio | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_ScottishSymphony/FelixMendelssohn-SymphonyNo.3InAMinorscottishOp.56-03-Adagio.mp3 |
| 81 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - I. Adagio Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-01-AdagioAllegro.mp3 |
| 82 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - II. Andante cantabile | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-02-AndanteCantabile.mp3 |
| 83 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - III. Minuetto Allegretto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-03-MinuettoAllegretto.mp3 |
| 84 | Wolfgang Amadeus Mozart | String Quartet No. 19 in C Major, K. 465 'Dissonance' - IV. Allegro molto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_StringQuartetNo.19inCMajorK465/WolfgangAmadeusMozart-StringQuartetNo.19InCK465Dissonance-04-AllegroVolto.mp3 |
| 85 | Franz Schubert | Sonata in A Major, D. 664 - I. Allegro moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMajorD.664/FranzSchubert-SonataInAMajorD.664-01-AllegroModerato.mp3 |
| 86 | Franz Schubert | Sonata in A Major, D. 664 - II. Andante | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMajorD.664/FranzSchubert-SonataInAMajorD.664-02-Andante.mp3 |
| 87 | Franz Schubert | Sonata in A Major, D. 664 - III. Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInAMajorD.664/FranzSchubert-SonataInAMajorD.664-03-Allegro.mp3 |
| 88 | Franz Schubert | Sonata in E-flat Major, D. 568 - I. Allegro moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-01-AllegroModerato.mp3 |
| 89 | Franz Schubert | Sonata in E-flat Major, D. 568 - II. Andante molto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-02-AndanteMolto.mp3 |
| 90 | Franz Schubert | Sonata in E-flat Major, D. 568 - III. Menuetto Allegretto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-03-MenuettoAllegretto.mp3 |
| 91 | Franz Schubert | Sonata in E-flat Major, D. 568 - IV. Allegro moderato | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInEFlatMajorD.568/FranzSchubert-SonataInEFlatMajorD.568-04-AllegroModerato.mp3 |
| 92 | Johannes Brahms | Symphony No. 2 in D Major, Op. 73 - I. Allegro non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.2inDMajor/JohannesBrahms-SymphonyNo.2InDMajorOp.73-01-AllegroNonTroppo.mp3 |
| 93 | Johannes Brahms | Symphony No. 2 in D Major, Op. 73 - II. Adagio non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.2inDMajor/JohannesBrahms-SymphonyNo.2InDMajorOp.73-02-AdagioNonToppo.mp3 |
| 94 | Johannes Brahms | Symphony No. 2 in D Major, Op. 73 - III. Allegretto grazioso | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.2inDMajor/JohannesBrahms-SymphonyNo.2InDMajorOp.73-03-AllegrettoGraziosotake1.mp3 |
| 95 | Josef Suk | Meditation | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Suk_Meditation/JosefSuk-Meditation.mp3 |
| 96 | Alexander Borodin | In the Steppes of Central Asia | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Borodin_InTheSteppesOfCentralAsia/AlexanderBorodin-InTheSteppesOfCentralAsia.mp3 |
| 97 | Felix Mendelssohn | Hebrides Overture 'Fingal's Cave' | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_Hebrides/FelixMendelssohn-HebridesOvertureFingalsCave.mp3 |
| 98 | Bedrich Smetana | Ma Vlast - Vltava | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Smetana_Vltava/BedichSmetana-MVlast-Vltava.mp3 |
| 99 | Wolfgang Amadeus Mozart | Symphony No. 40 in G Minor, K. 550 - II. Andante | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_SymphonyNo.40inGMinor/WolfgangAmadeusMozart-SymphonyNo.40InGMinorK.550-02-Andante.mp3 |
| 100 | Wolfgang Amadeus Mozart | Symphony No. 40 in G Minor, K. 550 - III. Menuetto Allegretto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mozart_SymphonyNo.40inGMinor/WolfgangAmadeusMozart-SymphonyNo.40InGMinorK.550-03-MenuettoAllegretto.mp3 |
| 101 | Felix Mendelssohn | String Quartet No. 6 in F Minor, Op. 80 - III. Adagio | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Mendelssohn_StringQuartetNo.6inFMinorOp.80/FelixMendelssohn-StringQuartetNo.6InFMinorOp.80-03-Adagio.mp3 |
| 102 | Franz Schubert | Sonata in D Major, D. 850 - II. Con moto | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInDMajorD.850/FranzSchubert-SonataInDMajorD.850-02-ConMoto.mp3 |
| 103 | Pyotr Ilyich Tchaikovsky | Symphony No. 6 in B Minor, Op. 74 'Pathetique' - IV. Finale Adagio lamentoso | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Tchaikovsky_SymphonyPathetique/PyotrIlyichTchaikovsky-SymphonyNo.6InBMinorOp.74pathtique-04-FinaleAdagioLamentoso.mp3 |
| 104 | Johannes Brahms | Symphony No. 1 in C Minor, Op. 68 - IV. Adagio - Piu andante - Allegro non troppo | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Brahms_SymphonyNo.1inCMinor/JohannesBrahms-SymphonyNo.1InCMinorOp.68-04-Adagio-PiAndante-AllegroNonTroppoMaConBrio.mp3 |
| 105 | Franz Schubert | Sonata in C Minor, D. 958 - III. Menuetto Allegro | Public Domain | https://archive.org/download/MusopenCollectionAsFlac/Schubert_SonataInCMinorD.958/FranzSchubert-SonataInCMinorD.958-03-MenuettoAllegro.mp3 |
| 106 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 1 in C major, BWV 846 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 01 Prelude No. 1 in C major, BWV 846.mp3 |
| 107 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 1 in C major, BWV 846 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 02 Fugue No. 1 in C major, BWV 846.mp3 |
| 108 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 2 in C minor, BWV 847 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 03 Prelude No. 2 in C minor, BWV 847.mp3 |
| 109 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 2 in C minor, BWV 847 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 04 Fugue No. 2 in C minor, BWV 847.mp3 |
| 110 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 3 in C-sharp major, BWV 848 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 05 Prelude No. 3 in C-sharp major, BWV 848.mp3 |
| 111 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 3 in C-sharp major, BWV 848 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 06 Fugue No. 3 in C-sharp major, BWV 848.mp3 |
| 112 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 4 in C-sharp minor, BWV 849 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 07 Prelude No. 4 in C-sharp minor, BWV 849.mp3 |
| 113 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 4 in C-sharp minor, BWV 849 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 08 Fugue No. 4 in C-sharp minor, BWV 849.mp3 |
| 114 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 5 in D major, BWV 850 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 09 Prelude No. 5 in D major, BWV 850.mp3 |
| 115 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 5 in D major, BWV 850 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 10 Fugue No. 5 in D major, BWV 850.mp3 |
| 116 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 6 in D minor, BWV 851 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 11 Prelude No. 6 in D minor, BWV 851.mp3 |
| 117 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 6 in D minor, BWV 851 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 12 Fugue No. 6 in D minor, BWV 851.mp3 |
| 118 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 7 in E-flat major, BWV 852 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 13 Prelude No. 7 in E-flat major, BWV 852.mp3 |
| 119 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 7 in E-flat major, BWV 852 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 14 Fugue No. 7 in E-flat major, BWV 852.mp3 |
| 120 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 8 in E-flat minor, BWV 853 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 15 Prelude No. 8 in E-flat minor, BWV 853.mp3 |
| 121 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 8 in D-sharp minor, BWV 853 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 16 Fugue No. 8 in D-sharp minor, BWV 853.mp3 |
| 122 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 9 in E major, BWV 854 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 17 Prelude No. 9 in E major, BWV 854.mp3 |
| 123 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 9 in E major, BWV 854 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 18 Fugue No. 9 in E major, BWV 854.mp3 |
| 124 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 10 in E minor, BWV 855 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 19 Prelude No. 10 in E minor, BWV 855.mp3 |
| 125 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 10 in E minor, BWV 855 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 20 Fugue No. 10 in E minor, BWV 855.mp3 |
| 126 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 11 in F major, BWV 856 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 21 Prelude No. 11 in F major, BWV 856.mp3 |
| 127 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 11 in F major, BWV 856 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 22 Fugue No. 11 in F major, BWV 856.mp3 |
| 128 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 12 in F minor, BWV 857 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 23 Prelude No. 12 in F minor, BWV 857.mp3 |
| 129 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 12 in F minor, BWV 857 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 24 Fugue No. 12 in F minor, BWV 857.mp3 |
| 130 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 13 in F-sharp major, BWV 858 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 25 Prelude No. 13 in F-sharp major, BWV 858.mp3 |
| 131 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 13 in F-sharp major, BWV 858 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 26 Fugue No. 13 in F-sharp major, BWV 858.mp3 |
| 132 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 14 in F-sharp minor, BWV 859 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 27 Prelude No. 14 in F-sharp minor, BWV 859.mp3 |
| 133 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 14 in F-sharp minor, BWV 859 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 28 Fugue No. 14 in F-sharp minor, BWV 859.mp3 |
| 134 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 15 in G major, BWV 860 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 29 Prelude No. 15 in G major, BWV 860.mp3 |
| 135 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 15 in G major, BWV 860 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 30 Fugue No. 15 in G major, BWV 860.mp3 |
| 136 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 16 in G minor, BWV 861 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 31 Prelude No. 16 in G minor, BWV 861.mp3 |
| 137 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 16 in G minor, BWV 861 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 32 Fugue No. 16 in G minor, BWV 861.mp3 |
| 138 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 17 in A-flat major, BWV 862 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 33 Prelude No. 17 in A-flat major, BWV 862.mp3 |
| 139 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 17 in A-flat major, BWV 862 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 34 Fugue No. 17 in A-flat major, BWV 862.mp3 |
| 140 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 18 in G-sharp minor, BWV 863 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 35 Prelude No. 18 in G-sharp minor, BWV 863.mp3 |
| 141 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 18 in G-sharp minor, BWV 863 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 36 Fugue No. 18 in G-sharp minor, BWV 863.mp3 |
| 142 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 19 in A major, BWV 864 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 37 Prelude No. 19 in A major, BWV 864.mp3 |
| 143 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 19 in A major, BWV 864 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 38 Fugue No. 19 in A major, BWV 864.mp3 |
| 144 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 20 in A minor, BWV 865 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 39 Prelude No. 20 in A minor, BWV 865.mp3 |
| 145 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 20 in A minor, BWV 865 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 40 Fugue No. 20 in A minor, BWV 865.mp3 |
| 146 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 21 in B-flat major, BWV 866 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 41 Prelude No. 21 in B-flat major, BWV 866.mp3 |
| 147 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 21 in B-flat major, BWV 866 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 42 Fugue No. 21 in B-flat major, BWV 866.mp3 |
| 148 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 22 in B-flat minor, BWV 867 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 43 Prelude No. 22 in B-flat minor, BWV 867.mp3 |
| 149 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 22 in B-flat minor, BWV 867 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 44 Fugue No. 22 in B-flat minor, BWV 867.mp3 |
| 150 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 23 in B major, BWV 868 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 45 Prelude No. 23 in B major, BWV 868.mp3 |
| 151 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 23 in B major, BWV 868 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 46 Fugue No. 23 in B major, BWV 868.mp3 |
| 152 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Prelude No. 24 in B minor, BWV 869 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 47 Prelude No. 24 in B minor, BWV 869.mp3 |
| 153 | J.S. Bach (Kimiko Ishizaka) | Well-Tempered Clavier Book I - Fugue No. 24 in B minor, BWV 869 | CC0 / PD Mark | https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko Ishizaka - Bach- Well-Tempered Clavier, Book 1 - 48 Fugue No. 24 in B minor, BWV 869.mp3 |
| 154 | Frederic Chopin | Nocturne Op. 9 No. 1 in B-flat minor | CC0 | https://archive.org/download/musopen-chopin/NocturneOp.9No.1InBFlatMinor.mp3 |
| 155 | Frederic Chopin | Nocturne Op. 9 No. 2 in E-flat major | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 9 no. 2 in E flat major.mp3 |
| 156 | Frederic Chopin | Nocturne Op. 9 No. 3 in B major | CC0 | https://archive.org/download/musopen-chopin/NocturneOp.9No.3.mp3 |
| 157 | Frederic Chopin | Nocturne Op. 15 No. 1 in F major | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 15 no. 1 In F major.mp3 |
| 158 | Frederic Chopin | Nocturne Op. 27 No. 1 in C-sharp minor | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 27 no. 1 in C sharp minor.mp3 |
| 159 | Frederic Chopin | Nocturne Op. 32 No. 1 in B major | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 32 no. 1 in B major.mp3 |
| 160 | Frederic Chopin | Nocturne Op. 32 No. 2 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 32 no. 2 in A flat major.mp3 |
| 161 | Frederic Chopin | Nocturne Op. 48 No. 1 in C minor | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 48 no. 1 in C minor.mp3 |
| 162 | Frederic Chopin | Nocturne Op. 48 No. 2 in F-sharp minor | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 48 no. 2 in F sharp minor.mp3 |
| 163 | Frederic Chopin | Nocturne Op. 55 No. 1 in F minor | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 55 no. 1 in F minor.mp3 |
| 164 | Frederic Chopin | Nocturne Op. 55 No. 2 in E-flat major | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 55 no. 2 in E flat major.mp3 |
| 165 | Frederic Chopin | Nocturne Op. 62 No. 2 in E major | CC0 | https://archive.org/download/musopen-chopin/Nocturne Op. 62 no. 2 in E major.mp3 |
| 166 | Frederic Chopin | Nocturne Op. 72 No. 1 in E minor | CC0 | https://archive.org/download/musopen-chopin/NocturneOp.72No.1InEMinor.mp3 |
| 167 | Frederic Chopin | Nocturne B. 108 in C minor | CC0 | https://archive.org/download/musopen-chopin/Nocturne B. 108 in C minor.mp3 |
| 168 | Frederic Chopin | Nocturne B. 49 in C-sharp minor 'Lento con gran espressione' | CC0 | https://archive.org/download/musopen-chopin/Nocturne B. 49 in C sharp minor 'Lento con gran espressione' (1).mp3 |
| 169 | Frederic Chopin | Nocturne Op. 27 No. 2 in D-flat major | CC0 | https://archive.org/download/musopen-chopin/NocturneOp27No2.mp3 |
| 170 | Frederic Chopin | Mazurka Op. 17 No. 3 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 17 no. 3 in A flat major.mp3 |
| 171 | Frederic Chopin | Mazurka Op. 17 No. 4 in A minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 17 no. 4 in A minor.mp3 |
| 172 | Frederic Chopin | Mazurka Op. 24 No. 4 in B-flat minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 24 no. 4 in B flat minor.mp3 |
| 173 | Frederic Chopin | Mazurka Op. 50 No. 3 in C-sharp minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 50 no. 3 in C sharp minor.mp3 |
| 174 | Frederic Chopin | Mazurka Op. 56 No. 1 in B major | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 56 no. 1 in B major.mp3 |
| 175 | Frederic Chopin | Mazurka Op. 56 No. 3 in C minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 56 no. 3 in C minor.mp3 |
| 176 | Frederic Chopin | Mazurka Op. 59 No. 1 in A minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 59 no. 1 in A minor.mp3 |
| 177 | Frederic Chopin | Mazurka Op. 59 No. 3 in F-sharp minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 59 no. 3 in F sharp minor.mp3 |
| 178 | Frederic Chopin | Mazurka Op. 50 No. 1 in G major | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 50 no. 1 in G major.mp3 |
| 179 | Frederic Chopin | Mazurka Op. 50 No. 2 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 50 no. 2 in A flat major.mp3 |
| 180 | Frederic Chopin | Mazurka Op. 7 No. 3 in F minor | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 7 no. 3 in F minor.mp3 |
| 181 | Frederic Chopin | Mazurka Op. 24 No. 3 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Mazurka Op. 24 no. 3 in A flat major.mp3 |
| 182 | Frederic Chopin | Waltz Op. 64 No. 2 in C-sharp minor | CC0 | https://archive.org/download/musopen-chopin/Waltz Op. 64 no. 2 in C sharp minor.mp3 |
| 183 | Frederic Chopin | Waltz Op. 69 No. 1 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Waltz Op. 69 no. 1 in A flat major.mp3 |
| 184 | Frederic Chopin | Waltz Op. 69 No. 2 in B minor | CC0 | https://archive.org/download/musopen-chopin/Waltz Op. 69 no. 2 in B minor.mp3 |
| 185 | Frederic Chopin | Waltz Op. 70 No. 2 in F minor | CC0 | https://archive.org/download/musopen-chopin/Waltz Op. 70 no. 2 in F minor.mp3 |
| 186 | Frederic Chopin | Waltz Op. 70 No. 3 in D-flat major | CC0 | https://archive.org/download/musopen-chopin/Waltz Op. 70 no. 3 in D flat major.mp3 |
| 187 | Frederic Chopin | Waltz Op. 34 No. 2 in A minor | CC0 | https://archive.org/download/musopen-chopin/WaltzOp.34No.2InAMinor.mp3 |
| 188 | Frederic Chopin | Waltz B. 46 in E-flat major | CC0 | https://archive.org/download/musopen-chopin/WaltzB.46InEFlatMajor.mp3 |
| 189 | Frederic Chopin | Waltz B. 56 in E minor | CC0 | https://archive.org/download/musopen-chopin/WaltzB.56InEMinor.mp3 |
| 190 | Frederic Chopin | Waltz Op. 34 No. 3 in F major | CC0 | https://archive.org/download/musopen-chopin/WaltzOp.34No.3InFMajor.mp3 |
| 191 | Frederic Chopin | Waltz B. 21 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/WaltzB.21InAFlatMajor.mp3 |
| 192 | Frederic Chopin | Fantaisie-Impromptu Op. 66 in C-sharp minor | CC0 | https://archive.org/download/musopen-chopin/Fantasie Impromptu Op. 66.mp3 |
| 193 | Frederic Chopin | Impromptu No. 1 Op. 29 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Impromptu no. 1 - Op. 29.mp3 |
| 194 | Frederic Chopin | Impromptu No. 2 Op. 36 in F-sharp major | CC0 | https://archive.org/download/musopen-chopin/Impromptu no. 2 - Op. 36.mp3 |
| 195 | Frederic Chopin | Impromptu No. 3 Op. 51 in G-flat major | CC0 | https://archive.org/download/musopen-chopin/Impromptu no. 3 - Op. 51.mp3 |
| 196 | Frederic Chopin | Prelude Op. 28 No. 6 in B minor | CC0 | https://archive.org/download/musopen-chopin/Prelude Op. 28 no. 6.mp3 |
| 197 | Frederic Chopin | Prelude Op. 28 No. 7 in A major | CC0 | https://archive.org/download/musopen-chopin/Prelude Op. 28 no. 7.mp3 |
| 198 | Frederic Chopin | Prelude Op. 28 No. 13 in F-sharp major | CC0 | https://archive.org/download/musopen-chopin/Prelude Op. 28 no. 13.mp3 |
| 199 | Frederic Chopin | Prelude Op. 28 No. 15 in D-flat major 'Raindrop' | CC0 | https://archive.org/download/musopen-chopin/Prelude Op. 28 no. 15.mp3 |
| 200 | Frederic Chopin | Prelude Op. 28 No. 17 in A-flat major | CC0 | https://archive.org/download/musopen-chopin/Prelude Op. 28 no. 17.mp3 |
| 201 | Frederic Chopin | Sonata for Piano and Cello, Op. 65 - III. Largo | CC0 | https://archive.org/download/musopen-chopin/Sonata for Piano and Cello in G Minor, Op. 65 - III. Largo.mp3 |

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
