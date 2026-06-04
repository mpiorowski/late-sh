// Static world definition for Lateania.
//
// Rooms and mob spawns are immutable data, loaded once into the service. This
// seed is 110 rooms spanning nine zones, a continuous descent from the hub town
// of Embergate down through forest, caverns, crypts, mines, an ice peak, a
// sunken citadel, and finally the demon realm of the Obsidian Throne. Each zone
// past the safe hub has regular mobs plus a named boss, scaled by tier.
//
// Zone layout (room id ranges):
//   1-5    Embergate (safe hub)            6-10   King's Road      (tier 1-2)
//   11-30  Whisperwood        (tier 2-3)   31-50  Duskhollow Caverns (tier 3-4)
//   51-65  Drowned Crypts     (tier 4-5)   66-80  Emberpeak Mines  (tier 5-6)
//   81-95  Frostspire Ascent  (tier 6-7)   96-105 The Sunken Citadel (tier 7-8)
//   106-110 The Obsidian Throne (tier 9-10, final boss Mal'gareth)
//
// Content is deliberately data, not code: `seed_world` hardcodes the world, but
// the shape (rooms keyed by id, exits as a direction map) is exactly what a
// future TOML/RON loader will produce. The current authored world has 198 rooms;
// the planned full design target remains 200.

use std::collections::HashMap;

use super::damage::{DamageProfile, DamageType};

/// Compass (with diagonals and vertical) directions a player can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    South,
    East,
    West,
    Northeast,
    Northwest,
    Southeast,
    Southwest,
    Up,
    Down,
}

impl Dir {
    pub fn label(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Northeast => "northeast",
            Self::Northwest => "northwest",
            Self::Southeast => "southeast",
            Self::Southwest => "southwest",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::North => "n",
            Self::South => "s",
            Self::East => "e",
            Self::West => "w",
            Self::Northeast => "ne",
            Self::Northwest => "nw",
            Self::Southeast => "se",
            Self::Southwest => "sw",
            Self::Up => "u",
            Self::Down => "d",
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
            Self::Northeast => Self::Southwest,
            Self::Southwest => Self::Northeast,
            Self::Northwest => Self::Southeast,
            Self::Southeast => Self::Northwest,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

pub type RoomId = u32;

/// A single location in the world: a node in the room graph.
#[derive(Clone, Debug)]
pub struct Room {
    pub id: RoomId,
    pub name: &'static str,
    pub desc: &'static str,
    pub zone: &'static str,
    pub exits: HashMap<Dir, RoomId>,
    /// True for towns and other no-combat zones.
    pub safe: bool,
}

/// A mob template that spawns at a home room.
#[derive(Clone, Debug)]
pub struct MobSpawn {
    pub id: u32,
    pub name: &'static str,
    pub home: RoomId,
    pub max_hp: i32,
    pub damage: i32,
    pub xp: i32,
    /// Seconds before a slain mob respawns.
    pub respawn_secs: u64,
    /// Item ids this mob can drop. Regular mobs have a chance at common gear;
    /// bosses are guaranteed to drop one item from a richer table.
    pub loot: &'static [u32],
    /// True for zone bosses: drops are guaranteed and announced loudly.
    pub boss: bool,
    /// Damage school dealt, plus resisted and weak schools, for interactive combat.
    pub profile: DamageProfile,
}

/// The immutable world: every room plus the mob roster.
#[derive(Clone, Debug)]
pub struct World {
    pub rooms: HashMap<RoomId, Room>,
    pub spawns: Vec<MobSpawn>,
    pub start_room: RoomId,
}

impl World {
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }
}

fn room(
    id: RoomId,
    name: &'static str,
    zone: &'static str,
    safe: bool,
    desc: &'static str,
    exits: &[(Dir, RoomId)],
) -> Room {
    Room {
        id,
        name,
        desc,
        zone,
        safe,
        exits: exits.iter().copied().collect(),
    }
}

/// Build the vertical-slice world: Embergate (safe hub) + the King's Road.
pub fn seed_world() -> World {
    let rooms = vec![
        room(
            1,
            "Embergate - Town Square",
            "Embergate",
            true,
            "Lanternlight pools on worn cobbles, and the great bronze brazier at the \
             square's heart throws a restless amber glow over the town that takes its \
             name from it. Embergate hums with evening trade: a fiddler saws by the \
             well, children chase a dog between the legs of off-duty guardsmen, and \
             the smell of the baker's last loaves hangs warm in the air. A notice \
             board leans by the well, thick with bounties and lost-cat pleas alike. \
             The Gilded Flagon glows north, the temple west, Market Row east, and the \
             South Gate and open road lie south.",
            &[
                (Dir::North, 2),
                (Dir::East, 3),
                (Dir::West, 4),
                (Dir::South, 5),
            ],
        ),
        room(
            2,
            "Embergate - The Gilded Flagon",
            "Embergate",
            true,
            "Woodsmoke, spilled ale, and roasting meat tangle in the air of the town's \
             beloved tavern. A great hearth roars at one end; long tables run with \
             candle-wax and carved initials. Adventurers swap tall tales over tankards, \
             a card game simmers toward a brawl in the corner, and the barkeep polishes \
             a horn cup that will never come clean. It is warm, loud, and safe - the \
             last of those rarer than the others. The square lies south.",
            &[(Dir::South, 1)],
        ),
        room(
            3,
            "Embergate - Market Row & the Ember Forge",
            "Embergate",
            true,
            "The lane narrows into a clamor of commerce, awnings snapping overhead and \
             barkers crying their wares. At the far end the open front of the Ember \
             Forge breathes furnace-heat into the street, where BRUNA IRONHAND, the \
             town smith, works a glowing billet with blows that ring off the rooftops. \
             Racks of blades, bows, and staves gleam at her shoulder, for sale to any \
             who can pay. The square lies west; the rest of the market district opens \
             east.",
            &[(Dir::West, 1), (Dir::East, 201)],
        ),
        room(
            4,
            "Embergate - Temple of the Dawn",
            "Embergate",
            true,
            "Pale columns rise toward a domed ceiling painted with a sunrise so vivid it \
             seems to warm the cold stone beneath. Clerics in white move in hushed \
             procession, and a hundred candles gutter at the feet of a gilded sun. Here \
             the wounded are mended and the dead are mourned; here, it is said, a fallen \
             adventurer's spirit is gathered up and returned to the world. A sense of \
             grave, patient mercy fills the air. The square lies east.",
            &[(Dir::East, 1)],
        ),
        room(
            5,
            "Embergate - South Gate",
            "Embergate",
            true,
            "A heavy iron portcullis stands raised. Beyond it the King's Road \
             stretches into open country. The square is north.",
            &[(Dir::North, 1), (Dir::South, 6)],
        ),
        // ---- Embergate shop district (safe) -----------------------------
        room(
            201,
            "Embergate - The Outfitter's Stall",
            "Embergate",
            true,
            "The market widens into a square of canvas stalls. Dominating it is the \
             Outfitter's, where TOMAS THREADNEEDLE presides over teetering heaps of \
             boiled leather, riveted mail, woven robes, and stout boots, all of it for \
             sale. He squints at every passerby as though measuring them for a coffin or \
             a cuirass, whichever they need first. The forge lies west; the lane runs on \
             north and east.",
            &[(Dir::West, 3), (Dir::North, 202), (Dir::East, 203)],
        ),
        room(
            202,
            "Embergate - The Apothecary",
            "Embergate",
            true,
            "A narrow shopfront crammed floor to rafter with bottles, jars, and bundled \
             herbs that fill the air with a sharp green reek. OLD MIRELA, bent nearly \
             double, shuffles between the shelves dispensing draughts and elixirs to any \
             with coin and an ailment. A cauldron mutters in the back. Nothing here is \
             quite labeled, but she always seems to know which bottle is which. The \
             outfitter's lies south.",
            &[(Dir::South, 201)],
        ),
        room(
            203,
            "Embergate - The Curio Cart",
            "Embergate",
            true,
            "A gaudy painted cart blocks half the lane, hung with charms, rings, and \
             trinkets that wink in the lanternlight. PELL THE MAGPIE leans against it \
             with a grin too wide to wholly trust, talking up the luck and virtue of his \
             wares to a skeptical crowd. Some of it may even be enchanted. The \
             outfitter's lies west; a quieter street runs east.",
            &[(Dir::West, 201), (Dir::East, 204)],
        ),
        room(
            204,
            "Embergate - The Bank of Embergate",
            "Embergate",
            true,
            "A squat, iron-doored building stands aloof from the market bustle, the only \
             stone-built shop on the row. Within, a humorless clerk tallies coin behind a \
             grille and a vault door broods at the back. Adventurers store their \
             hard-won gold here against the day a dungeon empties their purse. The curio \
             cart lies west; the town wall walk runs north.",
            &[(Dir::West, 203), (Dir::North, 205)],
        ),
        room(
            205,
            "Embergate - The Wall Walk",
            "Embergate",
            true,
            "Stone steps climb to a parapet atop the town wall, where a single guardsman \
             keeps a bored vigil over the dark country beyond. From here all of Embergate \
             spreads out below, lamplit and small and worth defending, and past the wall \
             the King's Road runs off into a night full of teeth. The bank lies back down \
             to the south.",
            &[(Dir::South, 204)],
        ),
        room(
            6,
            "The King's Road - Open Country",
            "King's Road",
            false,
            "The cobbles give way to packed earth. Tall grass whispers on either \
             side and the town wall recedes behind you to the north.",
            &[(Dir::North, 5), (Dir::South, 7)],
        ),
        room(
            7,
            "The King's Road - The Old Milestone",
            "King's Road",
            false,
            "A mossy milestone marks the leagues to far cities. A thin trail forks \
             east into a thicket; the road runs on south.",
            &[(Dir::North, 6), (Dir::East, 8), (Dir::South, 9)],
        ),
        room(
            8,
            "The King's Road - Bramble Thicket",
            "King's Road",
            false,
            "Thorns crowd a dead-end clearing. Something has trampled the grass \
             here recently. The trail back is west.",
            &[(Dir::West, 7)],
        ),
        room(
            9,
            "The King's Road - Ruined Watchtower",
            "King's Road",
            false,
            "A toppled watchtower slumps against the hillside, its stones scorched. \
             The road continues south into a shadowed defile; the way back is north.",
            &[(Dir::North, 7), (Dir::South, 10)],
        ),
        room(
            10,
            "The King's Road - The Defile",
            "King's Road",
            false,
            "Steep banks close in on a gloomy cut in the hills. The road ends \
             where a landslide once buried it; a narrow game-trail slips south \
             beneath leaning pines into older, darker country. The way back is \
             north.",
            &[(Dir::North, 9), (Dir::South, 11)],
        ),
        // ---- Whisperwood (forest, tier 2-3) -----------------------------
        room(
            11,
            "Whisperwood - The Threshold Oaks",
            "Whisperwood",
            false,
            "Two oaks older than the kingdom lean together to form a living arch, \
             their bark carved with charms so weathered they read only as scars. \
             The air past them is cooler, greener, and somehow listening. The \
             trail back climbs north toward the defile.",
            &[(Dir::North, 10), (Dir::South, 12)],
        ),
        room(
            12,
            "Whisperwood - Fernlight Hollow",
            "Whisperwood",
            false,
            "Sunlight falls in slow green coins through a canopy so high it feels \
             like a cathedral roof. Knee-deep ferns drink the light and hide the \
             ground entirely. Paths press south and east; the oaks stand north.",
            &[(Dir::North, 11), (Dir::South, 13), (Dir::East, 14)],
        ),
        room(
            13,
            "Whisperwood - The Murmuring Path",
            "Whisperwood",
            false,
            "The forest earns its name here: a wind you cannot feel moves the high \
             leaves in long sighing syllables, almost words. You keep turning to \
             answer someone who is not there. North and south the path runs on.",
            &[(Dir::North, 12), (Dir::South, 15)],
        ),
        room(
            14,
            "Whisperwood - The Toadstool Ring",
            "Whisperwood",
            false,
            "A perfect circle of scarlet toadstools rings a patch of unnaturally \
             soft moss. Old instinct tells you not to step inside it, and older \
             instinct tells you why. The hollow lies back to the west.",
            &[(Dir::West, 12)],
        ),
        room(
            15,
            "Whisperwood - The Leaning Birches",
            "Whisperwood",
            false,
            "Pale birches lean every direction at once, as though the ground had \
             shrugged a century ago and never settled. Their peeling bark hangs in \
             curls like discarded parchment. Ways lead north, south, and west.",
            &[(Dir::North, 13), (Dir::South, 16), (Dir::West, 17)],
        ),
        room(
            16,
            "Whisperwood - Wolf-Run Gully",
            "Whisperwood",
            false,
            "The land folds into a shallow gully floored with cracked mud, printed \
             over and over with the splayed tracks of a hunting pack. Tufts of grey \
             fur snag the bramble at nose height. The path continues north and south.",
            &[(Dir::North, 15), (Dir::South, 18)],
        ),
        room(
            17,
            "Whisperwood - The Hermit's Cairn",
            "Whisperwood",
            false,
            "A waist-high pile of river stones marks a grave no one tends. Someone \
             has balanced a single acorn on the topmost rock; it has not fallen, \
             though the wind worries everything else here. East returns to the birches.",
            &[(Dir::East, 15)],
        ),
        room(
            18,
            "Whisperwood - Spider-Silk Crossing",
            "Whisperwood",
            false,
            "Sheets of web span the gap between two dead elms, jeweled with dew and \
             the husks of things that stopped struggling long ago. The strands hum \
             faintly when you breathe. Paths lead north, south, and east.",
            &[(Dir::North, 16), (Dir::South, 19), (Dir::East, 20)],
        ),
        room(
            19,
            "Whisperwood - The Sunken Brook",
            "Whisperwood",
            false,
            "A clear brook has cut itself a channel so deep it runs below the roots, \
             chuckling in the dark a body's length beneath your feet. The forest \
             smells of cold stone and watercress. North and south the way goes on.",
            &[(Dir::North, 18), (Dir::South, 21)],
        ),
        room(
            20,
            "Whisperwood - The Weaver's Hollow",
            "Whisperwood",
            false,
            "Every branch in this dead-end hollow is strung with web until the trees \
             wear grey lace gowns. Small wrapped bundles turn slowly on invisible \
             threads. Nothing here is alive that should be. The crossing lies west.",
            &[(Dir::West, 18)],
        ),
        room(
            21,
            "Whisperwood - Stag-Horn Clearing",
            "Whisperwood",
            false,
            "Sun pours into a wide clearing where the bleached antlers of some \
             enormous stag rise from the grass like the rafters of a roofless hall. \
             Songbirds nest in the tines. The path runs north and south.",
            &[(Dir::North, 19), (Dir::South, 22)],
        ),
        room(
            22,
            "Whisperwood - The Crossroads Stone",
            "Whisperwood",
            false,
            "A moss-furred standing stone leans at the meeting of three trails, its \
             carved hand pointing nowhere that still exists. Offerings rot at its \
             base: bread, a copper ring, a child's wooden horse. Ways lead north, \
             south, and west.",
            &[(Dir::North, 21), (Dir::South, 23), (Dir::West, 24)],
        ),
        room(
            23,
            "Whisperwood - The Hanging Vale",
            "Whisperwood",
            false,
            "The trees thin over a vale where curtains of pale moss hang so thick \
             they brush your shoulders as you pass, cool and faintly damp, like the \
             hands of the polite dead. The path presses north and south.",
            &[(Dir::North, 22), (Dir::South, 25)],
        ),
        room(
            24,
            "Whisperwood - The Drowned Shrine",
            "Whisperwood",
            false,
            "A forgotten woodland shrine has sunk to its shoulders in black bog \
             water, only the carved face of some antlered god still breaking the \
             surface, watching the sky. Frogs go silent as you arrive. East returns \
             to the crossroads.",
            &[(Dir::East, 22)],
        ),
        room(
            25,
            "Whisperwood - The Char Circle",
            "Whisperwood",
            false,
            "A ring of trees stands black and branchless, killed by a fire that \
             never spread past their own trunks. In the center the ground is glassy \
             and warm. The forest leans away from this place. North and south remain.",
            &[(Dir::North, 23), (Dir::South, 26)],
        ),
        room(
            26,
            "Whisperwood - The Greenway Fork",
            "Whisperwood",
            false,
            "The undergrowth opens onto an ancient greenway, a road of turf so \
             straight it must have been laid by hands, now half-swallowed by the \
             forest reclaiming its own. Paths lead north, south, and east.",
            &[(Dir::North, 25), (Dir::South, 27), (Dir::East, 28)],
        ),
        room(
            27,
            "Whisperwood - The Lantern Trees",
            "Whisperwood",
            false,
            "Clusters of luminous fungus climb these trunks in spiral ladders, \
             casting a soft blue-green glow that makes the dusk beneath the canopy \
             into a perpetual underwater twilight. The way runs north and south.",
            &[(Dir::North, 26), (Dir::South, 29)],
        ),
        room(
            28,
            "Whisperwood - The Elder Grove",
            "Whisperwood",
            false,
            "At the heart of a ring of bowing trees stands one vast and ancient \
             treant, bark like cliff-stone, eyes like two cold green moons opening \
             slowly as you intrude on a silence kept for a thousand years. The \
             greenway lies west.",
            &[(Dir::West, 26)],
        ),
        room(
            29,
            "Whisperwood - The Root Stair",
            "Whisperwood",
            false,
            "The land tilts downward and the roots of the great trees arrange \
             themselves into a rough descending stair, slick with leaf-mould and \
             generations of fallen rain. Cold air rises from below. North climbs \
             back; south leads on.",
            &[(Dir::North, 27), (Dir::South, 30)],
        ),
        room(
            30,
            "Whisperwood - The Sinking Gate",
            "Whisperwood",
            false,
            "The forest floor opens at last into a sinkhole ringed by exposed roots, \
             a black throat breathing cave-cold air up into the green world. A rope \
             ladder, half-rotted, descends into the dark. North returns to the wood.",
            &[(Dir::North, 29), (Dir::Down, 31)],
        ),
        // ---- Duskhollow Caverns (caves & undead, tier 3-4) --------------
        room(
            31,
            "Duskhollow Caverns - The Drip Gallery",
            "Duskhollow Caverns",
            false,
            "Your boots find stone. The cavern mouth drips in slow, patient music, \
             each drop ringing in a darkness so complete your lantern seems an \
             apology. Daylight is a memory up the ladder, north and above. The cave \
             pushes south.",
            &[(Dir::Up, 30), (Dir::South, 32)],
        ),
        room(
            32,
            "Duskhollow Caverns - The Forking Throat",
            "Duskhollow Caverns",
            false,
            "The passage splits around a pillar of fused stalactite and stalagmite, \
             a stone hourglass taller than three men. Cold draughts breathe from \
             both branches. Ways lead north, south, and east.",
            &[(Dir::North, 31), (Dir::South, 33), (Dir::East, 34)],
        ),
        room(
            33,
            "Duskhollow Caverns - The Whispering Crawl",
            "Duskhollow Caverns",
            false,
            "The ceiling drops until you must stoop, and the walls press close \
             enough to scrape both shoulders. Your own breathing comes back to you \
             changed, as though the rock were trying the sound in its mouth. North \
             and south.",
            &[(Dir::North, 32), (Dir::South, 35)],
        ),
        room(
            34,
            "Duskhollow Caverns - The Ossuary Niche",
            "Duskhollow Caverns",
            false,
            "Someone stacked bones here, long ago and with terrible care: a wall of \
             skulls mortared with smaller bones, every empty socket aimed at the \
             room's one entrance. They have been waiting for company. West returns \
             to the fork.",
            &[(Dir::West, 32)],
        ),
        room(
            35,
            "Duskhollow Caverns - The Black Mirror",
            "Duskhollow Caverns",
            false,
            "A still pool fills the cavern floor, so utterly without ripple it \
             throws your lanternlight back like polished obsidian. Something pale \
             rests at the bottom, and you decide not to learn what. Ways lead north, \
             south, and west.",
            &[(Dir::North, 33), (Dir::South, 36), (Dir::West, 37)],
        ),
        room(
            36,
            "Duskhollow Caverns - The Stalactite Nave",
            "Duskhollow Caverns",
            false,
            "The chamber soars into a forest of hanging stone, fang upon fang \
             vanishing into a dark the lantern cannot reach. Drips fall from \
             impossible heights and burst cold against your neck. North and south \
             continue.",
            &[(Dir::North, 35), (Dir::South, 38)],
        ),
        room(
            37,
            "Duskhollow Caverns - The Sealed Door",
            "Duskhollow Caverns",
            false,
            "A door of iron-banded oak, swollen and black with damp, has been chained \
             shut from this side and then, for good measure, from this side again. \
             Something scratches the far face, slow and tireless. East returns to \
             the pool.",
            &[(Dir::East, 35)],
        ),
        room(
            38,
            "Duskhollow Caverns - The Crystal Vein",
            "Duskhollow Caverns",
            false,
            "A seam of clouded crystal threads the wall here, catching the lantern \
             and breaking it into a hundred trapped sparks that seem to drift like \
             slow snow inside the stone. It is beautiful and it is cold. Ways lead \
             north, south, and east.",
            &[(Dir::North, 36), (Dir::South, 39), (Dir::East, 40)],
        ),
        room(
            39,
            "Duskhollow Caverns - The Slumping Stair",
            "Duskhollow Caverns",
            false,
            "Steps cut by long-dead miners sag and slide underfoot, half-melted by \
             the patient creep of mineral water. Each one bears a worn carved \
             number in a counting-script no living tongue still speaks. North and \
             south.",
            &[(Dir::North, 38), (Dir::South, 41)],
        ),
        room(
            40,
            "Duskhollow Caverns - The Gnawed Larder",
            "Duskhollow Caverns",
            false,
            "Sacks and barrels rot in a side-chamber some lost expedition used for \
             stores. Everything organic has been gnawed to lace by teeth too \
             numerous and too small to think about. West returns to the vein.",
            &[(Dir::West, 38)],
        ),
        room(
            41,
            "Duskhollow Caverns - The Cold Hearth",
            "Duskhollow Caverns",
            false,
            "A ring of fire-blackened stones holds a heap of ash that has not felt \
             warmth in centuries, yet the air above it shimmers as though it \
             remembers being hot. Bedrolls lie around it, occupied by their owners \
             still. North and south.",
            &[(Dir::North, 39), (Dir::South, 42)],
        ),
        room(
            42,
            "Duskhollow Caverns - The Hanging Bridge",
            "Duskhollow Caverns",
            false,
            "A natural bridge of stone arches over a chasm whose bottom your lantern \
             never finds. Far below, something moves with a dragging, wet \
             deliberation. Best to cross quickly. Ways lead north, south, and west.",
            &[(Dir::North, 41), (Dir::South, 43), (Dir::West, 44)],
        ),
        room(
            43,
            "Duskhollow Caverns - The Fungal Garden",
            "Duskhollow Caverns",
            false,
            "Pale mushrooms grow waist-high in nightmare profusion, their caps \
             exhaling faint spores that prickle in the lungs and paint the lantern \
             with a sickly halo. Things have been harvesting them. North and south.",
            &[(Dir::North, 42), (Dir::South, 45)],
        ),
        room(
            44,
            "Duskhollow Caverns - The Throne of Bones",
            "Duskhollow Caverns",
            false,
            "A dead-end vault where the cavern floor rises into a dais, and upon it \
             a throne built entirely of fused skeletons leers in the lanternlight. \
             Its occupant lifts a crowned skull and regards you with two points of \
             cold blue fire. The bridge lies east.",
            &[(Dir::East, 42)],
        ),
        room(
            45,
            "Duskhollow Caverns - The Weeping Wall",
            "Duskhollow Caverns",
            false,
            "Mineral water sheets down a vast flowstone wall in an endless silver \
             curtain, and the sound is so like grief that you find your own throat \
             tightening for no reason you can name. North and south go on.",
            &[(Dir::North, 43), (Dir::South, 46)],
        ),
        room(
            46,
            "Duskhollow Caverns - The Echo Junction",
            "Duskhollow Caverns",
            false,
            "Five passages meet in a domed chamber that returns every sound \
             threefold, so that your single footstep becomes a marching company and \
             your whisper an argument. Ways lead north, south, and east.",
            &[(Dir::North, 45), (Dir::South, 47), (Dir::East, 48)],
        ),
        room(
            47,
            "Duskhollow Caverns - The Salt Flats",
            "Duskhollow Caverns",
            false,
            "An ancient sea died here and left its ghost: a flat white plain of \
             salt crust that crunches like thin ice underfoot, glittering to the \
             edge of the light. The air tastes of old oceans. North and south.",
            &[(Dir::North, 46), (Dir::South, 49)],
        ),
        room(
            48,
            "Duskhollow Caverns - The Miner's End",
            "Duskhollow Caverns",
            false,
            "A pick still stands buried in the dead-end wall where its owner left it, \
             and its owner left it because its owner is still here, slumped in the \
             corner, patient as the stone. West returns to the junction.",
            &[(Dir::West, 46)],
        ),
        room(
            49,
            "Duskhollow Caverns - The Drowned Stair",
            "Duskhollow Caverns",
            false,
            "Steps descend into black water that has risen to swallow them, and \
             keeps rising, drip by patient drip. The air grows colder and carries \
             the green reek of a flooded tomb. North climbs back; south wades on.",
            &[(Dir::North, 47), (Dir::South, 50)],
        ),
        room(
            50,
            "Duskhollow Caverns - The Sunken Arch",
            "Duskhollow Caverns",
            false,
            "A carved arch stands half-submerged at the cavern's lowest point, its \
             keystone graven with a drowned crown. Beyond and below it the water \
             becomes a flooded stair down into a deeper, older dark. North leads \
             back up.",
            &[(Dir::North, 49), (Dir::Down, 51)],
        ),
        // ---- Drowned Crypts (water & undead, tier 4-5) ------------------
        room(
            51,
            "Drowned Crypts - The Tide Vestibule",
            "Drowned Crypts",
            false,
            "You descend into a flooded hall where black water laps at carved \
             sarcophagi like moored boats. The cold is total and intimate, the kind \
             that settles in the marrow and stays. Up returns to the caverns; the \
             crypt runs south.",
            &[(Dir::Up, 50), (Dir::South, 52)],
        ),
        room(
            52,
            "Drowned Crypts - The Sarcophagus Row",
            "Drowned Crypts",
            false,
            "Stone coffins line both walls, their lids carved with the serene faces \
             of the long-dead. Several lids lie aside in the water. The faces beneath \
             are no longer serene. Ways lead north, south, and east.",
            &[(Dir::North, 51), (Dir::South, 53), (Dir::East, 54)],
        ),
        room(
            53,
            "Drowned Crypts - The Wading Nave",
            "Drowned Crypts",
            false,
            "The water rises to your thighs here, cold enough to ache, and things \
             brush your legs in the dark that you choose to believe are only weeds. \
             The current pulls gently south. North and south.",
            &[(Dir::North, 52), (Dir::South, 55)],
        ),
        room(
            54,
            "Drowned Crypts - The Reliquary",
            "Drowned Crypts",
            false,
            "Niches in this dead-end chamber once held holy relics; now they hold \
             only silt and the small bones of the desperate who came seeking them. \
             A single gold leaf still glints underwater. West returns to the row.",
            &[(Dir::West, 52)],
        ),
        room(
            55,
            "Drowned Crypts - The Black Font",
            "Drowned Crypts",
            false,
            "A great basin dominates the chamber, brimming with water blacker than \
             the dark around it. The surface holds a perfect, impossible stillness, \
             and your reflection in it is slow to copy your movements. North and \
             south.",
            &[(Dir::North, 53), (Dir::South, 56)],
        ),
        room(
            56,
            "Drowned Crypts - The Pillared Deep",
            "Drowned Crypts",
            false,
            "Rows of columns march off into water and darkness, each one carved as a \
             shrouded mourner, each one bowing slightly inward, so that to walk among \
             them is to be escorted by a procession of the grieving stone. Ways lead \
             north, south, and west.",
            &[(Dir::North, 55), (Dir::South, 57), (Dir::West, 58)],
        ),
        room(
            57,
            "Drowned Crypts - The Catafalque",
            "Drowned Crypts",
            false,
            "A raised bier stands clear of the flood, draped in rotted velvet that \
             still holds, somehow, a deep imperial purple. The body upon it is gone. \
             The shape pressed into the velvet suggests it merely rose and walked \
             away. North and south.",
            &[(Dir::North, 56), (Dir::South, 59)],
        ),
        room(
            58,
            "Drowned Crypts - The Oubliette",
            "Drowned Crypts",
            false,
            "A forgetting-hole: a dead-end shaft where prisoners were lowered and \
             the rope cut. The water here is deepest, and full of the patient, \
             upturned faces of everyone the crypt has ever swallowed. East returns \
             to the deep.",
            &[(Dir::East, 56)],
        ),
        room(
            59,
            "Drowned Crypts - The Choir of Salt",
            "Drowned Crypts",
            false,
            "Stalactites of crystallized brine hang in ranks like organ pipes, and \
             when the slow current stirs the flood they keen a single sustained note \
             that you feel in your teeth more than hear. North and south.",
            &[(Dir::North, 57), (Dir::South, 60)],
        ),
        room(
            60,
            "Drowned Crypts - The Sunken Crossing",
            "Drowned Crypts",
            false,
            "Submerged steps lead up onto a broad landing where three flooded halls \
             converge, their arches reflected in the still water until you cannot \
             tell stone from its double. Ways lead north, south, and east.",
            &[(Dir::North, 59), (Dir::South, 61), (Dir::East, 62)],
        ),
        room(
            61,
            "Drowned Crypts - The Pauper's Vault",
            "Drowned Crypts",
            false,
            "Here the dead were given no coffins, only shelves, and the shelves have \
             long since spilled their burden into the flood. The water is thick with \
             the anonymous dead, turning slowly in the current. North and south.",
            &[(Dir::North, 60), (Dir::South, 63)],
        ),
        room(
            62,
            "Drowned Crypts - The Lich's Sanctum",
            "Drowned Crypts",
            false,
            "The water falls away into a dry, candle-ringed sanctum where a robed \
             figure bends over a book bound in something that was once a face. It \
             does not turn. It says, in a voice like a closing tomb, that it has \
             been expecting you. The crossing lies west.",
            &[(Dir::West, 60)],
        ),
        room(
            63,
            "Drowned Crypts - The Weed-Choked Hall",
            "Drowned Crypts",
            false,
            "Pale subterranean weed has colonized this hall in drifting curtains, \
             feeding on the dead and on the dark, and it parts reluctantly as you \
             pass, closing again behind you like a held breath let go. North and south.",
            &[(Dir::North, 61), (Dir::South, 64)],
        ),
        room(
            64,
            "Drowned Crypts - The Last Lantern",
            "Drowned Crypts",
            false,
            "A bronze lantern hangs from the vaulted ceiling, and impossibly, \
             improbably, a small cold flame still burns within it, untended for \
             centuries. By its light the water ahead glitters with a different, \
             warmer mineral. North and south.",
            &[(Dir::North, 63), (Dir::South, 65)],
        ),
        room(
            65,
            "Drowned Crypts - The Ember Stair",
            "Drowned Crypts",
            false,
            "The flood drains away up a stair cut from raw red stone, and the air \
             changes utterly: drier, sharper, carrying the faraway tang of smoke and \
             hot metal. Something deep in the rock is awake and burning. North \
             returns to the crypts; up climbs toward the heat.",
            &[(Dir::North, 64), (Dir::Up, 66)],
        ),
        // ---- Emberpeak Mines (fire & dwarven ruin, tier 5-6) ------------
        room(
            66,
            "Emberpeak Mines - The Cinder Gate",
            "Emberpeak Mines",
            false,
            "You climb into a hewn hall where the very walls hold a sullen red \
             warmth, and runes carved by long-vanished dwarves still glow faintly in \
             the heat. Down leads back to the cold crypts; the mines open north.",
            &[(Dir::Down, 65), (Dir::North, 67)],
        ),
        room(
            67,
            "Emberpeak Mines - The Ore-Cart Junction",
            "Emberpeak Mines",
            false,
            "Rusted rails cross and recross the floor, and a single ore-cart sits \
             where it stopped an age ago, still heaped with raw red ingots no one \
             ever came to claim. The metal is warm to the touch. Ways lead south, \
             north, and east.",
            &[(Dir::South, 66), (Dir::North, 68), (Dir::East, 69)],
        ),
        room(
            68,
            "Emberpeak Mines - The Bellows Hall",
            "Emberpeak Mines",
            false,
            "Vast leather bellows, big as houses and cracked with age, flank a forge \
             channel cut into the floor. Far below, magma still pulses, and with \
             each pulse the dead bellows seem to stir, exhaling a gust of furnace \
             air. South and north.",
            &[(Dir::South, 67), (Dir::North, 70)],
        ),
        room(
            69,
            "Emberpeak Mines - The Collapsed Drift",
            "Emberpeak Mines",
            false,
            "A mining drift ends in a wall of fallen rubble, and pinned within it, \
             reaching, are the fossilized arms of the miners who did not get out. The \
             stone here ticks with trapped heat. West returns to the junction.",
            &[(Dir::West, 67)],
        ),
        room(
            70,
            "Emberpeak Mines - The Glass Foundry",
            "Emberpeak Mines",
            false,
            "The floor of this chamber is a frozen river of slag glass, swirled black \
             and red and gold, smooth enough to skate and just warm enough to remind \
             you what made it. Shapes are suspended within it. South and north.",
            &[(Dir::South, 68), (Dir::North, 71)],
        ),
        room(
            71,
            "Emberpeak Mines - The Anvil of Kings",
            "Emberpeak Mines",
            false,
            "A single anvil the size of a cart squats on a basalt plinth, its face \
             worn into a shallow valley by ten thousand vanished hands. Strike it and \
             the whole mountain answers in a low bronze hum. Ways lead south, north, \
             and west.",
            &[(Dir::South, 70), (Dir::North, 72), (Dir::West, 73)],
        ),
        room(
            72,
            "Emberpeak Mines - The Smelter's Gallery",
            "Emberpeak Mines",
            false,
            "Crucibles line a long gallery, each still cupping a disc of cooled \
             metal, each disc stamped with the seal of a dwarven house that no longer \
             exists in any memory but this one. The heat presses close. South and north.",
            &[(Dir::South, 71), (Dir::North, 74)],
        ),
        room(
            73,
            "Emberpeak Mines - The Slag Pit",
            "Emberpeak Mines",
            false,
            "Waste from a thousand years of smelting was tipped into this dead-end \
             pit, and it never fully cooled. A crust shifts over molten depths, and \
             the air above it bends with heat. Something basks half-submerged. East \
             returns to the anvil.",
            &[(Dir::East, 71)],
        ),
        room(
            74,
            "Emberpeak Mines - The Vein of Fire",
            "Emberpeak Mines",
            false,
            "A seam of raw firegold threads the wall, so hot it glows from within the \
             stone, lighting the chamber in a restless amber pulse like a heartbeat. \
             To mine it would be to mine a coal still burning. South and north.",
            &[(Dir::South, 72), (Dir::North, 75)],
        ),
        room(
            75,
            "Emberpeak Mines - The Cathedral Forge",
            "Emberpeak Mines",
            false,
            "The mine opens into a forge built like a temple, its central furnace a \
             chimney of carved stone rising beyond the lantern's reach. The dwarves \
             worshipped fire here, and fire, it seems, still attends. Ways lead south, \
             north, and east.",
            &[(Dir::South, 74), (Dir::North, 76), (Dir::East, 77)],
        ),
        room(
            76,
            "Emberpeak Mines - The Quenching Pools",
            "Emberpeak Mines",
            false,
            "Stone troughs that once cooled fresh-forged blades now hold black, \
             scummed water that steams without cease. The hiss is constant, almost a \
             voice, and the steam takes shapes you would rather it did not. South and north.",
            &[(Dir::South, 75), (Dir::North, 78)],
        ),
        room(
            77,
            "Emberpeak Mines - The Magma Heart",
            "Emberpeak Mines",
            false,
            "A dead-end cavern open to the mountain's molten core, a lake of fire \
             whose light hurts to look upon. From its surface a vast figure heaves \
             itself upright, basalt and lava, sloughing flame, turning a furnace gaze \
             upon the small cold thing that has entered its house. The forge lies west.",
            &[(Dir::West, 75)],
        ),
        room(
            78,
            "Emberpeak Mines - The Ascending Flue",
            "Emberpeak Mines",
            false,
            "A great chimney climbs the chamber, and the updraft through it is fierce \
             and hot, carrying sparks like upward-falling stars. Iron rungs set into \
             the flue lead toward a distant, paler light. South and north.",
            &[(Dir::South, 76), (Dir::North, 79)],
        ),
        room(
            79,
            "Emberpeak Mines - The Frost-Cracked Tunnel",
            "Emberpeak Mines",
            false,
            "Strangely, the heat fails here all at once, and the walls wear a rime of \
             frost that has no business this deep in a burning mountain. Your breath \
             fogs. Something cold is bleeding down from above. South and north.",
            &[(Dir::South, 78), (Dir::North, 80)],
        ),
        room(
            80,
            "Emberpeak Mines - The Rimeward Gate",
            "Emberpeak Mines",
            false,
            "The tunnel ends at a gate of fused ice and iron, beyond which a stair \
             climbs into killing cold and white light. Warm air dies against it. The \
             mines fall away south; up leads into winter.",
            &[(Dir::South, 79), (Dir::Up, 81)],
        ),
        // ---- Frostspire Ascent (ice mountain, tier 6-7) -----------------
        room(
            81,
            "Frostspire Ascent - The Threshold of Ice",
            "Frostspire Ascent",
            false,
            "You emerge onto a mountainside of blue glacial ice, and the cold takes \
             your breath as a physical theft. Wind screams past, carrying snow like \
             ground glass. Down returns to the warm dark; the ascent climbs north.",
            &[(Dir::Down, 80), (Dir::North, 82)],
        ),
        room(
            82,
            "Frostspire Ascent - The Wind-Carved Pass",
            "Frostspire Ascent",
            false,
            "The path threads a pass where the wind has sculpted the ice into a \
             gallery of blades and figures, frozen courtiers bowing eternally to a \
             gale that never tires of them. Ways lead south, north, and east.",
            &[(Dir::South, 81), (Dir::North, 83), (Dir::East, 84)],
        ),
        room(
            83,
            "Frostspire Ascent - The Glass Stair",
            "Frostspire Ascent",
            false,
            "Steps of clear ice climb the slope, and through them you can see down \
             into the glacier's heart, where dark shapes are frozen at depths no \
             summer will ever reach. Do not look too long. South and north.",
            &[(Dir::South, 82), (Dir::North, 85)],
        ),
        room(
            84,
            "Frostspire Ascent - The Frozen Caravan",
            "Frostspire Ascent",
            false,
            "A merchant train lies where the cold caught it: ponies, carts, and \
             huddled drivers all locked in clear ice, perfectly preserved, their last \
             expressions still legible. A dead-end, and a warning. West returns to \
             the pass.",
            &[(Dir::West, 82)],
        ),
        room(
            85,
            "Frostspire Ascent - The Singing Crevasse",
            "Frostspire Ascent",
            false,
            "A crevasse splits the path, and the wind crossing its mouth draws from \
             the depths a sound between a flute and a scream, rising and falling, a \
             song the mountain has practiced for ten thousand winters. South and north.",
            &[(Dir::South, 83), (Dir::North, 86)],
        ),
        room(
            86,
            "Frostspire Ascent - The Aurora Shelf",
            "Frostspire Ascent",
            false,
            "A broad ice shelf opens to the sky, and overhead the aurora pours in \
             silent rivers of green and violet light, painting the snow in colors \
             that have no warmth in them at all. Ways lead south, north, and west.",
            &[(Dir::South, 85), (Dir::North, 87), (Dir::West, 88)],
        ),
        room(
            87,
            "Frostspire Ascent - The Hoarfrost Shrine",
            "Frostspire Ascent",
            false,
            "A shrine to some forgotten winter-god stands sheathed in feathered \
             hoarfrost, its offering-bowl heaped with frozen coins and the frozen \
             hands of those who lingered to leave them. The cold here has intent. \
             South and north.",
            &[(Dir::South, 86), (Dir::North, 89)],
        ),
        room(
            88,
            "Frostspire Ascent - The Wendigo's Larder",
            "Frostspire Ascent",
            false,
            "A dead-end ice cave hung with frozen carcasses, neatly butchered, \
             neatly stored, by something that understands winter and is patient with \
             it. Not all the carcasses are animals. The shelf lies east.",
            &[(Dir::East, 86)],
        ),
        room(
            89,
            "Frostspire Ascent - The Knife-Edge Ridge",
            "Frostspire Ascent",
            false,
            "The path narrows to a spine of wind-scoured ice with a killing drop to \
             either hand, the whole world falling away into white cloud below. You \
             cross it one careful step at a time. South and north.",
            &[(Dir::South, 87), (Dir::North, 90)],
        ),
        room(
            90,
            "Frostspire Ascent - The Sky Altar",
            "Frostspire Ascent",
            false,
            "A flat shelf near the summit holds an altar of black stone, the only \
             dark thing in all this white, swept perpetually clear of snow by a wind \
             that seems to serve it. Ways lead south, north, and east.",
            &[(Dir::South, 89), (Dir::North, 91), (Dir::East, 92)],
        ),
        room(
            91,
            "Frostspire Ascent - The Last Camp",
            "Frostspire Ascent",
            false,
            "A ring of frozen tents marks where some expedition made its final stand \
             against the mountain. The cold preserved everything: the banked fire, \
             the open journals, the climbers in their bags, sleeping the sleep that \
             does not end. South and north.",
            &[(Dir::South, 90), (Dir::North, 93)],
        ),
        room(
            92,
            "Frostspire Ascent - The Wyrm's Eyrie",
            "Frostspire Ascent",
            false,
            "A dead-end hollow scoured into the peak itself, floored with the picked \
             bones of centuries of prey. Ice crusts the walls in great raked furrows. \
             Something vast and white uncoils from the frost, and the storm itself \
             seems to draw breath. The altar lies west.",
            &[(Dir::West, 90)],
        ),
        room(
            93,
            "Frostspire Ascent - The Cloud-Breaking Stair",
            "Frostspire Ascent",
            false,
            "The stair climbs through the cloud-deck at last, and breaks above it \
             into a thin, brilliant, freezing sunlight, the whole storm reduced to a \
             white sea churning beneath your feet. South and north.",
            &[(Dir::South, 91), (Dir::North, 94)],
        ),
        room(
            94,
            "Frostspire Ascent - The Summit Approach",
            "Frostspire Ascent",
            false,
            "The peak is close now, a black needle of stone breaking through the ice, \
             and set into its base is a doorway too straight and too dark to be \
             natural, exhaling a cold that even the mountain does not own. South and north.",
            &[(Dir::South, 93), (Dir::North, 95)],
        ),
        room(
            95,
            "Frostspire Ascent - The Sunken Gate",
            "Frostspire Ascent",
            false,
            "A vast gate of black basalt stands half-buried in the summit ice, its \
             lintel carved with a citadel that should not be here, on a peak, at the \
             top of the world. The way in leads down, into stone, into the past. \
             South returns to the snow.",
            &[(Dir::South, 94), (Dir::Up, 96)],
        ),
        // ---- The Sunken Citadel (megadungeon, tier 7-8) -----------------
        room(
            96,
            "The Sunken Citadel - The Hall of Entry",
            "The Sunken Citadel",
            false,
            "You pass from ice into a hall of black stone so vast the lantern cannot \
             find its roof, and the cold here is not winter's cold but something \
             older and more deliberate. The gate is down and behind; the citadel \
             opens north.",
            &[(Dir::Down, 95), (Dir::North, 97)],
        ),
        room(
            97,
            "The Sunken Citadel - The Gallery of Kings",
            "The Sunken Citadel",
            false,
            "Statues of armored kings line a processional gallery, each twice the \
             height of a man, each with its carved face deliberately, completely \
             chiseled away. Whatever they ruled wished them forgotten. Ways lead \
             south, north, and east.",
            &[(Dir::South, 96), (Dir::North, 98), (Dir::East, 99)],
        ),
        room(
            98,
            "The Sunken Citadel - The Shattered Rotunda",
            "The Sunken Citadel",
            false,
            "A domed chamber lies cracked open by some ancient cataclysm, its mosaic \
             floor depicting a war between things with too many wings, half of it \
             fallen into a chasm that swallowed the rest of the story. South and north.",
            &[(Dir::South, 97), (Dir::North, 100)],
        ),
        room(
            99,
            "The Sunken Citadel - The Reliquary of Saints",
            "The Sunken Citadel",
            false,
            "Glass cases line this dead-end vault, each meant to hold a holy bone, \
             each shattered from within. Whatever sainthood was kept here did not \
             stay dead, and did not stay holy. West returns to the gallery.",
            &[(Dir::West, 97)],
        ),
        room(
            100,
            "The Sunken Citadel - The Drowned Throne Room",
            "The Sunken Citadel",
            false,
            "Black water fills the lower half of a throne room built for giants, and \
             the throne itself rises from the flood, empty, its arms gripped by \
             skeletal hands that did not belong to whoever last sat there. South and north.",
            &[(Dir::South, 98), (Dir::North, 101)],
        ),
        room(
            101,
            "The Sunken Citadel - The Iron Library",
            "The Sunken Citadel",
            false,
            "Books bound in beaten iron fill shelves three storeys high, their pages \
             metal leaf, their words etched in a script that hurts to focus on. Some \
             volumes are chained shut. Some chains have been broken outward. Ways lead \
             south, north, and west.",
            &[(Dir::South, 100), (Dir::North, 102), (Dir::West, 103)],
        ),
        room(
            102,
            "The Sunken Citadel - The Orrery Vault",
            "The Sunken Citadel",
            false,
            "A great brass orrery hangs broken in the dark, its planets stilled \
             mid-orbit, and the constellation it models is no sky you have ever seen \
             or would wish to. One sphere, black and unlabeled, still slowly turns. \
             South and north.",
            &[(Dir::South, 101), (Dir::North, 104)],
        ),
        room(
            103,
            "The Sunken Citadel - The Oath-Breaker's Cell",
            "The Sunken Citadel",
            false,
            "A dead-end chapel-cell where a paladin was once walled up alive for a \
             sin the citadel would not name. The wall is broken now, from the inside, \
             and the figure that kneels in the rubble lifts a ruined helm and a \
             blackened sword. The library lies east.",
            &[(Dir::East, 101)],
        ),
        room(
            104,
            "The Sunken Citadel - The Gallery of Whispers",
            "The Sunken Citadel",
            false,
            "A long hall where the black stone has been worked into ten thousand \
             carved mouths, all open, and as you pass each one breathes a single word \
             of a sentence ten thousand years long that no one was ever meant to hear \
             the end of. South and north.",
            &[(Dir::South, 102), (Dir::North, 105)],
        ),
        room(
            105,
            "The Sunken Citadel - The Obsidian Descent",
            "The Sunken Citadel",
            false,
            "The floor falls away into a stair of polished obsidian spiraling down \
             into a red-lit dark, and the heat that rises from below is not fire's \
             heat but the warmth of something vast and living and awake. South leads \
             back; down leads to the throne beneath.",
            &[(Dir::South, 104), (Dir::Down, 106)],
        ),
        // ---- The Obsidian Throne (endgame demon realm, tier 9-10) -------
        room(
            106,
            "The Obsidian Throne - The Threshold of Embers",
            "The Obsidian Throne",
            false,
            "You step into a realm that is no longer stone but something between \
             flesh and volcanic glass, and it is warm, and it pulses, and it knows \
             you are here. The stair climbs up behind you toward the world; the \
             throne-realm spreads south.",
            &[(Dir::Up, 105), (Dir::South, 107)],
        ),
        room(
            107,
            "The Obsidian Throne - The Avenue of the Damned",
            "The Obsidian Throne",
            false,
            "A wide black road runs between two endless rows of the bound damned, \
             figures of ash and ember frozen mid-scream, lighting your way with the \
             dull red glow of their own slow burning. They turn their heads to watch \
             you pass. North and south.",
            &[(Dir::North, 106), (Dir::South, 108)],
        ),
        room(
            108,
            "The Obsidian Throne - The Court of Cinders",
            "The Obsidian Throne",
            false,
            "A vast antechamber where lesser demons hold a mockery of court, perched \
             on thrones of cooling lava, their attention turning to you all at once \
             like a hundred furnace doors swinging open. Ways lead north, south, and east.",
            &[(Dir::North, 107), (Dir::South, 109), (Dir::East, 110)],
        ),
        room(
            109,
            "The Obsidian Throne - The Well of Souls",
            "The Obsidian Throne",
            false,
            "A dead-end shaft plunges into a red abyss, and from it rises a column \
             of the screaming, swirling damned, an updraft of agony that lights the \
             whole chamber the color of a wound. The court lies north.",
            &[(Dir::North, 108)],
        ),
        room(
            110,
            "The Obsidian Throne - The Throne of Mal'gareth",
            "The Obsidian Throne",
            false,
            "The world ends in a chamber of black glass and red fire, and upon a \
             throne grown from the realm itself sits the Archdemon Mal'gareth, vast \
             and patient and terribly amused, rising now to its full and dreadful \
             height to greet the mortal who came so very far only to kneel. The court \
             lies west.",
            &[(Dir::West, 108)],
        ),
    ];

    let spawns = vec![
        MobSpawn {
            id: 1,
            name: "a scrawny goblin",
            home: 6,
            max_hp: 18,
            damage: 3,
            xp: 12,
            respawn_secs: 30,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 2,
            name: "a road bandit",
            home: 8,
            max_hp: 26,
            damage: 5,
            xp: 20,
            respawn_secs: 45,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 3,
            name: "a gaunt wolf",
            home: 9,
            max_hp: 22,
            damage: 4,
            xp: 16,
            respawn_secs: 40,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        // ---- Whisperwood (tier 2-3) -------------------------------------
        MobSpawn {
            id: 10,
            name: "a snarling wolf",
            home: 16,
            max_hp: 30,
            damage: 6,
            xp: 26,
            respawn_secs: 45,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 11,
            name: "a giant forest spider",
            home: 18,
            max_hp: 34,
            damage: 7,
            xp: 30,
            respawn_secs: 50,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Poison, None, None),
        },
        MobSpawn {
            id: 12,
            name: "a bog-rotted corpse",
            home: 24,
            max_hp: 38,
            damage: 6,
            xp: 32,
            respawn_secs: 50,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Boss: Whisperwood
        MobSpawn {
            id: 13,
            name: "the Elder Treant",
            home: 28,
            max_hp: 120,
            damage: 12,
            xp: 150,
            respawn_secs: 300,
            loot: &[1006, 1201, 1301],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Physical),
                Some(DamageType::Fire),
            ),
        },
        // ---- Duskhollow Caverns (tier 3-4) ------------------------------
        MobSpawn {
            id: 20,
            name: "a clattering skeleton",
            home: 34,
            max_hp: 44,
            damage: 8,
            xp: 40,
            respawn_secs: 55,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 21,
            name: "a cave lurker",
            home: 40,
            max_hp: 50,
            damage: 9,
            xp: 46,
            respawn_secs: 55,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(DamageType::Physical, None, None),
        },
        MobSpawn {
            id: 22,
            name: "a grave-cold wraith",
            home: 48,
            max_hp: 54,
            damage: 10,
            xp: 52,
            respawn_secs: 60,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Boss: Duskhollow Caverns
        MobSpawn {
            id: 23,
            name: "the Bone Tyrant",
            home: 44,
            max_hp: 180,
            damage: 16,
            xp: 220,
            respawn_secs: 300,
            loot: &[1105, 1202, 1302],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // ---- Drowned Crypts (tier 4-5) ----------------------------------
        MobSpawn {
            id: 30,
            name: "a drowned revenant",
            home: 54,
            max_hp: 60,
            damage: 11,
            xp: 60,
            respawn_secs: 60,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 31,
            name: "a crypt ghoul",
            home: 58,
            max_hp: 66,
            damage: 12,
            xp: 66,
            respawn_secs: 60,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 32,
            name: "a pale drowned thing",
            home: 61,
            max_hp: 70,
            damage: 13,
            xp: 72,
            respawn_secs: 65,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // Boss: Drowned Crypts
        MobSpawn {
            id: 33,
            name: "the Lich Vael",
            home: 62,
            max_hp: 240,
            damage: 20,
            xp: 320,
            respawn_secs: 360,
            loot: &[1008, 1204, 1302],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // ---- Emberpeak Mines (tier 5-6) ---------------------------------
        MobSpawn {
            id: 40,
            name: "a molten husk",
            home: 69,
            max_hp: 78,
            damage: 14,
            xp: 80,
            respawn_secs: 65,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        MobSpawn {
            id: 41,
            name: "a forge-wight",
            home: 72,
            max_hp: 84,
            damage: 15,
            xp: 88,
            respawn_secs: 70,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        MobSpawn {
            id: 42,
            name: "an ember salamander",
            home: 73,
            max_hp: 90,
            damage: 16,
            xp: 96,
            respawn_secs: 70,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        // Boss: Emberpeak Mines
        MobSpawn {
            id: 43,
            name: "the Magma Colossus",
            home: 77,
            max_hp: 320,
            damage: 26,
            xp: 440,
            respawn_secs: 360,
            loot: &[1009, 1205, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Frost),
            ),
        },
        // ---- Frostspire Ascent (tier 6-7) -------------------------------
        MobSpawn {
            id: 50,
            name: "a frost-bound revenant",
            home: 84,
            max_hp: 96,
            damage: 17,
            xp: 104,
            respawn_secs: 70,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        MobSpawn {
            id: 51,
            name: "a rime-clawed wendigo",
            home: 88,
            max_hp: 104,
            damage: 19,
            xp: 116,
            respawn_secs: 75,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        MobSpawn {
            id: 52,
            name: "an ice-wraith",
            home: 91,
            max_hp: 110,
            damage: 20,
            xp: 124,
            respawn_secs: 75,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // Boss: Frostspire Ascent
        MobSpawn {
            id: 53,
            name: "the Wyrm of Frostspire",
            home: 92,
            max_hp: 420,
            damage: 32,
            xp: 600,
            respawn_secs: 420,
            loot: &[1007, 1205, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Frost,
                Some(DamageType::Frost),
                Some(DamageType::Fire),
            ),
        },
        // ---- The Sunken Citadel (tier 7-8) ------------------------------
        MobSpawn {
            id: 60,
            name: "a faceless sentinel",
            home: 99,
            max_hp: 120,
            damage: 22,
            xp: 140,
            respawn_secs: 80,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Physical),
                Some(DamageType::Arcane),
            ),
        },
        MobSpawn {
            id: 61,
            name: "an iron-bound horror",
            home: 100,
            max_hp: 130,
            damage: 24,
            xp: 152,
            respawn_secs: 80,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Physical,
                Some(DamageType::Physical),
                Some(DamageType::Arcane),
            ),
        },
        MobSpawn {
            id: 62,
            name: "a whispering shade",
            home: 104,
            max_hp: 140,
            damage: 26,
            xp: 164,
            respawn_secs: 85,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Boss: The Sunken Citadel
        MobSpawn {
            id: 63,
            name: "the Fallen Paladin",
            home: 103,
            max_hp: 520,
            damage: 38,
            xp: 820,
            respawn_secs: 420,
            loot: &[1109, 1202, 1304],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Holy,
                Some(DamageType::Physical),
                Some(DamageType::Shadow),
            ),
        },
        // ---- The Obsidian Throne (tier 9-10) ----------------------------
        MobSpawn {
            id: 70,
            name: "a cinder fiend",
            home: 107,
            max_hp: 160,
            damage: 30,
            xp: 200,
            respawn_secs: 90,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 71,
            name: "a lava-throned demon",
            home: 108,
            max_hp: 180,
            damage: 33,
            xp: 230,
            respawn_secs: 90,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Fire,
                Some(DamageType::Fire),
                Some(DamageType::Holy),
            ),
        },
        MobSpawn {
            id: 72,
            name: "a soul-wracked horror",
            home: 109,
            max_hp: 200,
            damage: 36,
            xp: 260,
            respawn_secs: 95,
            loot: &[1000, 1100, 1103, 1300],
            boss: false,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
        // Final boss
        MobSpawn {
            id: 73,
            name: "the Archdemon Mal'gareth",
            home: 110,
            max_hp: 800,
            damage: 48,
            xp: 1500,
            respawn_secs: 600,
            loot: &[1009, 1205, 1401],
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Shadow),
                Some(DamageType::Holy),
            ),
        },
    ];

    let mut rooms: HashMap<RoomId, Room> = rooms.into_iter().map(|r| (r.id, r)).collect();
    let mut spawns = spawns;

    // Append the deeper-exploration wings (rooms 300+), reciprocal by construction.
    extend_world(&mut rooms, &mut spawns);

    World {
        rooms,
        spawns,
        start_room: 1,
    }
}

// ---- World extension wings (the path from 115 to 200 rooms) ---------------
//
// Each wing is a chain of rooms branching off an existing "anchor" room into a
// zone, linked head-to-tail. Links are wired in BOTH directions here, so a wing
// can never produce a one-way exit (the class of bug hand-authoring is prone
// to). Wing room ids start at 300 to stay clear of the base world.

/// One room in a wing: its name, description, and the direction that leads
/// DEEPER (to the next room in the chain). The return link is added automatically.
struct WingRoom {
    name: &'static str,
    desc: &'static str,
    /// Direction from this room to the next in the chain.
    onward: Dir,
}

/// Link two rooms reciprocally: `from` gets `dir` -> `to`, `to` gets the
/// opposite back to `from`. Never overwrites an existing exit.
fn link(rooms: &mut HashMap<RoomId, Room>, from: RoomId, dir: Dir, to: RoomId) {
    if let Some(r) = rooms.get_mut(&from) {
        r.exits.entry(dir).or_insert(to);
    }
    if let Some(r) = rooms.get_mut(&to) {
        r.exits.entry(dir.opposite()).or_insert(from);
    }
}

/// Append a chain of wing rooms to `rooms`, anchored to `anchor` via `entry`
/// (the direction from the anchor into the wing's first room). Returns the id of
/// the wing's last (deepest) room so callers can place a boss/mob there.
fn add_wing(
    rooms: &mut HashMap<RoomId, Room>,
    zone: &'static str,
    safe: bool,
    anchor: RoomId,
    entry: Dir,
    start_id: RoomId,
    chain: &[WingRoom],
) -> RoomId {
    let mut prev = anchor;
    let mut prev_dir = entry;
    let mut id = start_id;
    for wing in chain {
        rooms.insert(
            id,
            Room {
                id,
                name: wing.name,
                desc: wing.desc,
                zone,
                exits: HashMap::new(),
                safe,
            },
        );
        link(rooms, prev, prev_dir, id);
        prev = id;
        prev_dir = wing.onward;
        id += 1;
    }
    id - 1
}

fn wr(name: &'static str, desc: &'static str, onward: Dir) -> WingRoom {
    WingRoom { name, desc, onward }
}

fn extend_world(rooms: &mut HashMap<RoomId, Room>, spawns: &mut Vec<MobSpawn>) {
    let mut next_mob: u32 = 300;
    let mut mob = |spawns: &mut Vec<MobSpawn>,
                   name: &'static str,
                   home: RoomId,
                   hp: i32,
                   dmg: i32,
                   xp: i32,
                   boss: bool,
                   loot: &'static [u32],
                   profile: DamageProfile| {
        let id = next_mob;
        next_mob += 1;
        spawns.push(MobSpawn {
            id,
            name,
            home,
            max_hp: hp,
            damage: dmg,
            xp,
            respawn_secs: if boss { 320 } else { 55 },
            loot,
            boss,
            profile,
        });
    };

    fn p(at: DamageType, res: Option<DamageType>, weak: Option<DamageType>) -> DamageProfile {
        DamageProfile::new(at, res, weak)
    }
    use DamageType as D;

    // Each wing: (zone, anchor, entry dir, onward dir, id base, rooms). Id bases
    // are 30 apart so a wing can grow to 30 rooms without colliding. Mobs are
    // placed relative to the captured start/end ids, never hardcoded.

    // ---- Whisperwood: The Sunken Glade (12 rooms) -----------------------
    let start = 300;
    let last = add_wing(
        rooms,
        "Whisperwood",
        false,
        14,
        Dir::North,
        start,
        &[
            wr(
                "Whisperwood - The Mushroom Stair",
                "Shelves of bracket-fungus climb a slope like a giant's staircase, soft and cold underfoot, spores drifting in the lanternlight. North; the ring lies south.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Glowcap Grotto",
                "A hollow beneath an upturned root glimmers with luminous caps in blue and green, a drowned dreamlike light over soft loam. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Toadstool Court",
                "Rings within rings of fungus carpet a clearing, and the longer you stand the more you feel watched by things at ankle height. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Weeping Willow",
                "A willow vast as a tower trails its branches to the ground, and the wind in them makes a sound exactly like a woman crying. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Bog Causeway",
                "A path of half-sunk logs crosses a black bog that breathes bubbles and worse. Stepping wrong here is a quiet way to vanish. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Drowned Oak",
                "An oak has fallen full-length into the bog and rotted into a hollow tunnel; you walk through the inside of a dead giant. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Witch's Hut",
                "A crooked hut leans on chicken-scratch foundations, windows dark, door ajar on a single creaking hinge. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Hag's Garden",
                "Behind the hut a garden grows things no garden should: pale gourds with faces, vines that flinch from the light. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Bone Orchard",
                "Trees here have grown around old bones until trunk and skeleton are one, and the fruit they bear is best left unpicked. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Moonwell",
                "A perfectly round well brims with water that glows faintly silver, reflecting a moon not in tonight's sky. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Whispering Stones",
                "A ring of leaning stones mutters among themselves, falling silent the instant you turn to listen. North.",
                Dir::North,
            ),
            wr(
                "Whisperwood - The Sunken Glade",
                "The trees draw back from a circle of green where a single shaft of moonlight falls, beautiful and far too quiet, where something has waited a very long time. The way back is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a will-o'-wisp",
        start + 1,
        26,
        6,
        24,
        false,
        COMMON_LOOT,
        p(D::Fire, None, Some(D::Frost)),
    );
    mob(
        spawns,
        "a giant glowcap spider",
        start + 5,
        34,
        7,
        30,
        false,
        COMMON_LOOT,
        p(D::Poison, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a bog-mire lurker",
        start + 8,
        40,
        8,
        36,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Poison), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Hexcrone of the Glade",
        last,
        130,
        13,
        165,
        true,
        &[1006, 1201, 1302],
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );

    // ---- Duskhollow: The Barrow Deep (11 rooms) -------------------------
    let start = 330;
    let last = add_wing(
        rooms,
        "Duskhollow Caverns",
        false,
        37,
        Dir::West,
        start,
        &[
            wr(
                "Duskhollow - Behind the Sealed Door",
                "The chained door gives onto a passage no light has touched in centuries, the air dead and close and faintly sweet with old decay. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Gravewater Pool",
                "Black water fills a basin to the brim, pale shapes drifting just beneath its skin, neither sunk nor surfaced. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Creeping Dark",
                "The lantern seems to shrink here, the dark pressing in close enough to feel, patient and almost fond. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Hall of Urns",
                "Thousands of clay urns line shelves to the unseen ceiling, each holding forgotten ash. Many are broken, their contents not where they should be. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Mourner's Stair",
                "Steps worn into a smooth trough by centuries of grieving feet descend into a deeper cold. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Catacomb Maze",
                "Passages branch and rejoin among walls of stacked bone until direction loses meaning; only the draught from ahead keeps you true. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Lamentation Hall",
                "A vast chamber where the slightest sound returns as a chorus of weeping, until you cannot tell the echo from the dead. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Gilded Tomb",
                "A single tomb of beaten gold gleams untouched by the rot, its lid carved with a sleeping king who is no longer inside. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Guardian's Rest",
                "Stone sentinels line the final approach, each with a real sword rusted into its carved hands, each having taken one step from its plinth. West.",
                Dir::West,
            ),
            wr(
                "Duskhollow - The Barrow King's Vault",
                "A burial chamber fit for a king who refused the grave: gold heaped in the dark, and at its center a throne where a crowned and withered thing turns its head. The way out is east.",
                Dir::West,
            ),
        ],
    );
    mob(
        spawns,
        "a tomb-rat swarm",
        start + 1,
        38,
        7,
        30,
        false,
        COMMON_LOOT,
        p(D::Physical, None, Some(D::Fire)),
    );
    mob(
        spawns,
        "a grave moth cloud",
        start + 3,
        44,
        8,
        38,
        false,
        COMMON_LOOT,
        p(D::Poison, None, Some(D::Holy)),
    );
    mob(
        spawns,
        "a shambling barrow-guard",
        start + 6,
        52,
        9,
        48,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "a clutch of bonepickers",
        start + 8,
        56,
        10,
        54,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Shadow), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Barrow King",
        last,
        190,
        17,
        235,
        true,
        &[1105, 1202, 1302],
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );

    // ---- Drowned Crypts: The Tidal Catacombs (11 rooms) -----------------
    let start = 360;
    let last = add_wing(
        rooms,
        "Drowned Crypts",
        false,
        54,
        Dir::South,
        start,
        &[
            wr(
                "Drowned Crypts - The Brine Stair",
                "Salt-crusted steps spiral down into water that rises to meet you, cold as a drowned bell. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Coral Ossuary",
                "Bone and pale coral have grown into one another until you cannot tell which the dead were and which the sea made. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Kelp Forest",
                "Ropes of black kelp rise from the flooded dark and sway though there is no current, parting reluctantly as you wade. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Sunken Chapel",
                "A chapel stands fully submerged, pews in drowned rows, its altar candle somehow trailing a thread of smoke up through the water. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Pearl Vault",
                "Drowned treasure spills from broken chests, every coin and pearl furred with the same pale rot. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Anemone Garden",
                "Things that might be flowers and might be mouths carpet the walls, opening and closing in slow patient unison. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Siren's Landing",
                "A dry shelf above the flood holds a single carved seat facing the water, where something once sat to sing ships down. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Black Trench",
                "The floor falls away into a trench whose bottom the lantern never finds, and from which a slow cold current breathes. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Bone Reef",
                "A reef built entirely of the bones of the drowned rises in pale ramparts, and things nest in its hollows. South.",
                Dir::South,
            ),
            wr(
                "Drowned Crypts - The Leviathan's Maw",
                "A vast flooded cavern dominated by the rib-cage of something that should not fit in any sea, and in its shadow a drowned horror stirs. The way back is north.",
                Dir::South,
            ),
        ],
    );
    mob(
        spawns,
        "a drowned acolyte",
        start + 1,
        58,
        11,
        60,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "a kelp-strangler",
        start + 3,
        64,
        12,
        66,
        false,
        COMMON_LOOT,
        p(D::Poison, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "a reef-thing",
        start + 6,
        70,
        13,
        72,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "a brine-bloated drowned",
        start + 8,
        74,
        13,
        76,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );
    mob(
        spawns,
        "the Tide-Drowned Leviathan",
        last,
        260,
        21,
        340,
        true,
        &[1008, 1204, 1302],
        p(D::Frost, Some(D::Frost), Some(D::Lightning)),
    );

    // ---- Emberpeak: The Deep Forge (11 rooms) ---------------------------
    let start = 390;
    let last = add_wing(
        rooms,
        "Emberpeak Mines",
        false,
        69,
        Dir::North,
        start,
        &[
            wr(
                "Emberpeak - The Cleared Drift",
                "Fresh rubble dragged aside; beyond it the dwarven tunnels run on, hot and red-lit. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Ore Sorters",
                "Conveyor troughs of cold black iron still hold their last sorted heaps of glittering ore, untouched for an age. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Gem Cutters' Hall",
                "Workbenches stand abandoned mid-task, half-cut gems clamped in vices, catching the forge-light like trapped sparks. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Molten Channel",
                "A river of slow magma crosses the hall in a stone trough, and the air above it shimmers hard enough to bend the sight. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Bellows Engine",
                "A vast machine of leather and iron still wheezes faintly, breathing furnace-air into tunnels no one tends. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Slag Cathedral",
                "Waste glass and slag have been stacked into soaring buttresses, a cathedral built by accident over a thousand years of work. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Runesmith's Sanctum",
                "Walls of dwarven runes pulse with banked heat, and a forge of black iron broods at the heart, never gone cold. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Ash Vault",
                "Knee-deep grey ash fills a sealed vault, and something has been writing in it, over and over, the same dwarven word for sorry. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Firewalk",
                "A narrow bridge crosses a lake of fire, the stone underfoot warm enough to feel through boots. North.",
                Dir::North,
            ),
            wr(
                "Emberpeak - The Heart of the Forge",
                "The deepest forge of all, open to a vein of living magma, where a guardian of fused slag and fire heaves itself upright. The way out is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a coal-wretch",
        start + 1,
        80,
        14,
        84,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "a cinder-imp",
        start + 3,
        84,
        14,
        86,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "a runeforged sentry",
        start + 6,
        88,
        15,
        90,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Physical), Some(D::Frost)),
    );
    mob(
        spawns,
        "a slag golem",
        start + 8,
        94,
        16,
        96,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Fire), Some(D::Frost)),
    );
    mob(
        spawns,
        "the Forgeheart Guardian",
        last,
        340,
        27,
        460,
        true,
        &[1009, 1205, 1304],
        p(D::Fire, Some(D::Fire), Some(D::Frost)),
    );

    // ---- Frostspire: The Glacier's Heart (11 rooms) ---------------------
    let start = 420;
    let last = add_wing(
        rooms,
        "Frostspire Ascent",
        false,
        84,
        Dir::North,
        start,
        &[
            wr(
                "Frostspire - The Blue Descent",
                "A stair carved into the glacier itself plunges into translucent blue depths, the cold deepening with every step. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Frozen Falls",
                "A waterfall caught mid-plunge forms a curtain of clear ice three storeys high, and behind it, dimly, something moves. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Rime Galleries",
                "Halls of ice branch in every direction, their walls so clear you see the frozen dark of the glacier's interior pressing close. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Mammoth Graveyard",
                "Tusked giants lie where the ice took them an age ago, perfectly kept, their great frozen eyes still open. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Aurora Cavern",
                "Light from the surface filters down through fathoms of ice and breaks into slow drifting color across the cavern floor. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Frostbound Hoard",
                "A dragon's hoard sheathed entirely in clear ice, every coin and crown visible and utterly unreachable. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Silent Crevasse",
                "A crack in the glacier so deep the cold pouring from it stops your breath, and the silence is total enough to hear your own heart. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Wyrm's Spine",
                "You walk the frozen length of some titanic serpent locked in the ice, scale after scale underfoot for a hundred paces. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Last Warmth",
                "A geothermal vent has kept one small chamber bearable, and the bones around the dead fire say others found it too late. North.",
                Dir::North,
            ),
            wr(
                "Frostspire - The Glacier's Heart",
                "At the glacier's frozen core, a chamber of impossible blue holds an elder ice-wyrm coiled in eternal sleep, waking now, slow and vast and furious. The way back is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a frost-bound wretch",
        start + 1,
        100,
        17,
        106,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "an ice-stalker",
        start + 3,
        104,
        18,
        110,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "a glacial revenant",
        start + 6,
        110,
        18,
        116,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );
    mob(
        spawns,
        "a hoarfrost wraith",
        start + 8,
        114,
        19,
        120,
        false,
        COMMON_LOOT,
        p(D::Frost, Some(D::Physical), Some(D::Fire)),
    );
    mob(
        spawns,
        "the Heart-of-Winter Wyrm",
        last,
        440,
        33,
        620,
        true,
        &[1007, 1205, 1304],
        p(D::Frost, Some(D::Frost), Some(D::Fire)),
    );

    // ---- Sunken Citadel: The Forbidden Wing (10 rooms) ------------------
    let start = 450;
    let last = add_wing(
        rooms,
        "The Sunken Citadel",
        false,
        99,
        Dir::North,
        start,
        &[
            wr(
                "Citadel - The Sealed Wing",
                "A wing the citadel tried to wall away from itself, the bricks bulging outward as though something pushed from within. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Mirror Gallery",
                "Black mirrors line a hall, and your reflection is always a half-second late and, you slowly realize, not always copying what you do. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Forgotten Archive",
                "Shelves of iron books stand toppled and burned, and the ash still holds the shape of words that hurt to almost-read. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Astronomer's Tower",
                "A ruined observatory open to a sky full of wrong stars, its brass telescope aimed at a darkness that seems to aim back. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Hall of Hands",
                "Ten thousand carved stone hands reach from the walls, and as you pass, the nearest ones slowly, gently, turn to follow. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Drowned Laboratory",
                "Flooded benches hold apparatus of glass and bone, and things in jars track you with eyes that should not still be wet. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Whispering Crypt",
                "The carved mouths of the citadel reach their loudest here, all speaking the last word of the long sentence at once. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Throne of Echoes",
                "An empty throne faces a hall built to carry a single voice forever; the air still trembles faintly with the last command given. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Vault of Saints",
                "Sarcophagi of the citadel's holy dead stand cracked open from within, their occupants risen to a sanctity gone sour. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Antechamber of the Heart",
                "The black stone turns warm and almost soft here, and the lantern dims as though something ahead is drinking the light. North.",
                Dir::North,
            ),
            wr(
                "Citadel - The Sealed Heart",
                "The forbidden room at the citadel's core, where a being of folded shadow and starlight unfurls from the dark it was bound in. The way out is south.",
                Dir::North,
            ),
        ],
    );
    mob(
        spawns,
        "a hollow archivist",
        start + 2,
        122,
        22,
        144,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Physical), Some(D::Holy)),
    );
    mob(
        spawns,
        "a mirror-wraith",
        start + 4,
        128,
        23,
        150,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Physical), Some(D::Holy)),
    );
    mob(
        spawns,
        "a grasping hand-swarm",
        start + 6,
        132,
        24,
        156,
        false,
        COMMON_LOOT,
        p(D::Physical, Some(D::Physical), Some(D::Arcane)),
    );
    mob(
        spawns,
        "the Warden of the Sealed Heart",
        last,
        540,
        39,
        840,
        true,
        &[1109, 1202, 1304],
        p(D::Shadow, Some(D::Shadow), Some(D::Holy)),
    );

    // ---- Obsidian Throne: The Infernal Depths (10 rooms) ----------------
    let start = 480;
    let last = add_wing(
        rooms,
        "The Obsidian Throne",
        false,
        109,
        Dir::South,
        start,
        &[
            wr(
                "Obsidian Throne - The Burning Descent",
                "A stair of cooling lava leads down into a heat that is almost a sound, a low roar at the edge of hearing. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Furnace of Sins",
                "Vast furnaces line a hall where the damned are unmade and remade, screaming on a loop ten thousand years long. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Chained Legion",
                "Rank upon rank of bound demons stand frozen at attention, and ten thousand burning eyes track you down the length of the hall. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Pact Chamber",
                "A round room of black glass where bargains were struck with the throne itself; the contracts still hang in the air, written in light, waiting. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The River of Fire",
                "A true river of flame crosses the dark, and a ferryman of ash waits at its bank with an open, expectant hand. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Gallery of Torments",
                "Each alcove holds a single damned soul in eternal, inventive agony, and each turns its head to beg you for an end. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Brimstone Bridge",
                "A bridge of fused bone arches over an abyss that glows the deep red of a banked forge, exhaling sulphur. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Hall of Broken Oaths",
                "Shattered contracts litter the floor, and the air is thick with the ghosts of promises the throne was glad to see broken. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Weeping Pits",
                "Pits of black tar bubble and sigh, and each rising bubble briefly wears a face that mouths a name before it bursts. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Antechamber of the Abyss",
                "The realm thins toward something worse, the black glass going translucent on a void that has no bottom and no patience. South.",
                Dir::South,
            ),
            wr(
                "Obsidian Throne - The Abyssal Gate",
                "The realm bottoms out at a gate into pure abyss, guarded by a herald of Mal'gareth who will not let a soul pass either way. The way back is north.",
                Dir::South,
            ),
        ],
    );
    mob(
        spawns,
        "a chained tormentor",
        start + 2,
        168,
        30,
        206,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "a tormented soul-husk",
        start + 4,
        174,
        31,
        212,
        false,
        COMMON_LOOT,
        p(D::Shadow, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "an ash ferryman",
        start + 6,
        182,
        32,
        222,
        false,
        COMMON_LOOT,
        p(D::Fire, Some(D::Fire), Some(D::Holy)),
    );
    mob(
        spawns,
        "the Herald of Mal'gareth",
        last,
        620,
        43,
        1100,
        true,
        &[1009, 1205, 1401],
        p(D::Shadow, Some(D::Fire), Some(D::Holy)),
    );

    // ---- King's Road: The Bandit Trail (9 rooms, low-level detour) ------
    let start = 510;
    let last = add_wing(
        rooms,
        "King's Road",
        false,
        8,
        Dir::East,
        start,
        &[
            wr(
                "King's Road - The Poacher's Trail",
                "A narrow trail worn by furtive feet winds east through the brush, snares glinting in the undergrowth. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Hollow Tree",
                "A hollow oak big enough to shelter in has been used as exactly that; a cold campfire and gnawed bones say by whom. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Abandoned Farmstead",
                "A burned-out farm slumps in a clearing, its fields gone to weed, its well gone to black water. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Scarecrow Field",
                "Rags on crossed sticks lean at wrong angles across a dead field, and you count one more of them on the way out than on the way in. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Crossroads Gibbet",
                "An iron gibbet creaks at a forgotten crossroads, its occupant long since flown to bone. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Smuggler's Cellar",
                "A trapdoor in the ruin of an inn drops to a cellar of stolen goods, half of it spoiled, all of it watched. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Watchpost",
                "A half-built bandit watchpost overlooks the trail, its lookout's stool still warm, its lookout suddenly not in sight. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Camp Approach",
                "The trees thin toward firelight and rough laughter; you are clearly expected, and clearly not welcome. East.",
                Dir::East,
            ),
            wr(
                "King's Road - The Bandit Camp",
                "A ring of tattered tents around a guttering fire marks the lair of the road's bandit crew, and their chief rises, hand on hilt, to greet the fool who found them. The way back is west.",
                Dir::East,
            ),
        ],
    );
    mob(
        spawns,
        "a feral poacher's hound",
        start + 1,
        26,
        5,
        22,
        false,
        COMMON_LOOT,
        DamageProfile::physical(),
    );
    mob(
        spawns,
        "a road cutthroat",
        start + 4,
        30,
        6,
        24,
        false,
        COMMON_LOOT,
        DamageProfile::physical(),
    );
    mob(
        spawns,
        "a crossbow bandit",
        start + 6,
        32,
        7,
        28,
        false,
        COMMON_LOOT,
        DamageProfile::physical(),
    );
    mob(
        spawns,
        "the Bandit Chief Garrote",
        last,
        110,
        12,
        130,
        true,
        &[1006, 1201, 1301],
        DamageProfile::physical(),
    );
}

/// Common low-tier drop pool shared by wandering wing mobs.
const COMMON_LOOT: &[u32] = &[1000, 1100, 1103, 1300];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exit_resolves_to_a_real_room() {
        let world = seed_world();
        for room in world.rooms.values() {
            for (dir, target) in &room.exits {
                assert!(
                    world.rooms.contains_key(target),
                    "room {} ({}) has a {} exit to missing room {}",
                    room.id,
                    room.name,
                    dir.label(),
                    target
                );
            }
        }
    }

    #[test]
    fn exits_are_reciprocal_where_expected() {
        // Embergate square (1) <-> south gate (5): going south then north returns.
        let world = seed_world();
        let square = world.room(1).expect("square exists");
        let gate_id = square.exits.get(&Dir::South).copied().expect("south exit");
        let gate = world.room(gate_id).expect("gate exists");
        assert_eq!(gate.exits.get(&Dir::North).copied(), Some(1));
    }

    #[test]
    fn start_room_exists_and_is_safe() {
        let world = seed_world();
        let start = world.room(world.start_room).expect("start room exists");
        assert!(start.safe, "players should spawn in a safe room");
    }

    #[test]
    fn world_has_expected_size_and_every_mob_homes_to_a_real_room() {
        let world = seed_world();
        assert_eq!(world.rooms.len(), 198, "expected 198 authored rooms");
        for spawn in &world.spawns {
            assert!(
                world.rooms.contains_key(&spawn.home),
                "mob {} ({}) homes to missing room {}",
                spawn.id,
                spawn.name,
                spawn.home
            );
        }
    }

    #[test]
    fn there_are_at_least_fifty_distinct_enemy_types() {
        let world = seed_world();
        let mut names: Vec<&str> = world.spawns.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert!(
            names.len() >= 50,
            "expected 50+ distinct enemy types, found {}",
            names.len()
        );
    }

    #[test]
    fn mob_spawn_ids_are_unique() {
        let world = seed_world();
        let mut ids: Vec<u32> = world.spawns.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(count, ids.len(), "duplicate mob spawn id");
    }

    #[test]
    fn every_boss_has_a_guaranteed_loot_table() {
        let world = seed_world();
        let bosses: Vec<_> = world.spawns.iter().filter(|s| s.boss).collect();
        assert!(bosses.len() >= 7, "expected at least 7 zone bosses");
        for boss in bosses {
            assert!(!boss.loot.is_empty(), "boss {} has no loot", boss.name);
            for id in boss.loot {
                assert!(
                    crate::app::door::lateania::items::item(*id).is_some(),
                    "boss {} drops missing item {}",
                    boss.name,
                    id
                );
            }
        }
    }

    #[test]
    fn all_mob_loot_references_real_items() {
        let world = seed_world();
        for spawn in &world.spawns {
            for id in spawn.loot {
                assert!(
                    crate::app::door::lateania::items::item(*id).is_some(),
                    "mob {} drops missing item {}",
                    spawn.name,
                    id
                );
            }
        }
    }

    #[test]
    fn every_room_reachable_from_start() {
        let world = seed_world();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![world.start_room];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(room) = world.room(id) {
                for target in room.exits.values() {
                    stack.push(*target);
                }
            }
        }
        assert_eq!(
            seen.len(),
            world.rooms.len(),
            "some rooms are unreachable from the start room"
        );
    }
}
