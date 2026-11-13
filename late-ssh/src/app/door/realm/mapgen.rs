//! Generated worlds: a realm map built from a handful of numbers instead of
//! from Natural Earth data.
//!
//! The contract with the rest of realm is that a generated map is *derivable*.
//! A game stores a `GeneratedMapSpec` — a seed, three counts, and the list of
//! names — and the grid is rebuilt from it on demand, identically, forever. A
//! 400-territory world is a megabyte of cells; storing that per game in JSONB
//! would be storing something we can recompute in a few hundred milliseconds
//! and cache. Names are the one part that cannot be recomputed (they may come
//! from the AI service, which is not a pure function of the seed), so names
//! are the one part that is persisted.
//!
//! The globe is the same globe as Earth: the same equirectangular projection
//! over the same sphere, so kilometres mean the same thing on a generated map
//! as on the real one and every distance-tuned ruleset constant
//! (`sea_hop_km`, the distance decay) carries over untouched. What changes is
//! where the land is, not how big the world is. Total land is held near
//! Earth's 29% for the same reason: the parameters should change the shape of
//! a game, not silently change how much there is to take.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::app::common::worldmap::data::{
    Territory, TerritoryGrid, TerritoryId, WATER, WorldMap, build_mips, compute_bboxes,
    compute_coastal, compute_landmasses, compute_sea_lanes, compute_weights,
};

/// Grid resolution of a generated world. A quarter of Earth's linear
/// resolution: the coastlines here are invented rather than surveyed, so the
/// detail Earth needs to keep Gambia visible buys nothing, and a megabyte a
/// world keeps the cache cheap enough to hold every live game's map at once.
pub const GEN_WIDTH: u16 = 1024;
pub const GEN_HEIGHT: u16 = 512;

pub const GEN_MIN_TERRITORIES: u16 = 50;
pub const GEN_MAX_TERRITORIES: u16 = 400;
pub const GEN_MAX_CONTINENTS: u8 = 10;
pub const GEN_MAX_ISLANDS: u8 = 20;

/// Earth's land fraction, held constant across every parameter combination.
const LAND_FRACTION: f64 = 0.29;
/// How much of the land the continents take when there are islands too. The
/// rest is shared out between the islands, which is what keeps twenty islands
/// a scattering of small places rather than a second set of continents.
const CONTINENT_SHARE: f64 = 0.88;
const EARTH_RADIUS_KM: f64 = 6371.0;
/// Nothing is generated above this latitude. Not a physical rule — an
/// equirectangular grid stretches the poles across the whole width, so a
/// polar country is a stripe on screen and a rounding error in area.
const MAX_LATITUDE: f64 = 72.0;

/// Everything needed to rebuild a generated world. Serialised into the game's
/// frozen ruleset snapshot, so it is as immutable as the rules are.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedMapSpec {
    pub seed: u64,
    pub continents: u8,
    pub islands: u8,
    pub territories: u16,
    /// How the world is drawn from the rest of the spec. Part of the recipe:
    /// a world opened under one generator is rebuilt by it for life.
    #[serde(default = "Generator::written_before_relief")]
    pub generator: Generator,
    /// Territory names in id order. Empty (or short) falls back to the
    /// built-in pool, which is what keeps this working with no AI key and
    /// deterministic in tests.
    #[serde(default)]
    pub names: Vec<String>,
}

/// The algorithm that turns a spec into a world. A generated world is rebuilt
/// from its spec on every load, so changing how worlds are drawn has to be a
/// new variant beside the old one, never an edit to it: an edit redraws every
/// live world under its players' feet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Generator {
    /// Each landmass flooded outward from its own centre, centres spread as
    /// far apart as they go, countries cut by four-way flood. Kept for the
    /// worlds opened under it (season 2); delete it once none is live.
    Flood,
    /// A relief: every landmass a rise in one elevation field, the sea level
    /// set so the land is Earth's share, continents strung off one another
    /// and islands off their coasts, countries cut by an eight-way flood over
    /// rough ground. What every world is opened with now (`MAPS.md`).
    Relief,
}

impl Generator {
    /// Specs stored before the field existed were all drawn by `Flood`.
    fn written_before_relief() -> Self {
        Self::Flood
    }

    fn tag(self) -> u8 {
        match self {
            Self::Flood => 0,
            Self::Relief => 1,
        }
    }
}

impl GeneratedMapSpec {
    /// Clamp a spec into the ranges the generator can actually build. A world
    /// with no land at all is the one combination that has to be refused
    /// rather than clamped, so zero continents *and* zero islands becomes a
    /// single continent.
    pub fn normalized(&self) -> Self {
        let continents = self.continents.min(GEN_MAX_CONTINENTS);
        let islands = self.islands.min(GEN_MAX_ISLANDS);
        let (continents, islands) = if continents == 0 && islands == 0 {
            (1, 0)
        } else {
            (continents, islands)
        };
        Self {
            seed: self.seed,
            continents,
            islands,
            territories: self
                .territories
                .clamp(GEN_MIN_TERRITORIES, GEN_MAX_TERRITORIES),
            generator: self.generator,
            names: self.names.clone(),
        }
    }

    /// Cache key: two specs with the same key build the same grid. Names are
    /// in it because they are part of the built map, not just a label on it.
    pub fn key(&self) -> u64 {
        let spec = self.normalized();
        let mut hasher = blake3::Hasher::new();
        hasher.update(&spec.seed.to_le_bytes());
        hasher.update(&[spec.continents, spec.islands]);
        hasher.update(&spec.territories.to_le_bytes());
        hasher.update(&[spec.generator.tag()]);
        for name in &spec.names {
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
        }
        let bytes = hasher.finalize();
        u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap_or([0; 8]))
    }

    /// The one-line shape of this world, for the create dialog and the board.
    pub fn summary(&self) -> String {
        let spec = self.normalized();
        let continents = match spec.continents {
            0 => "no mainland".to_string(),
            1 => "1 continent".to_string(),
            n => format!("{n} continents"),
        };
        let islands = match spec.islands {
            0 => "no islands".to_string(),
            1 => "1 island".to_string(),
            n => format!("{n} islands"),
        };
        format!("{continents} · {islands} · {} lands", spec.territories)
    }
}

impl Default for GeneratedMapSpec {
    fn default() -> Self {
        Self {
            seed: 0,
            continents: 5,
            islands: 8,
            territories: 300,
            generator: Generator::Relief,
            names: Vec::new(),
        }
    }
}

// ----- the cache

/// Built worlds, keyed by spec. Every session looking at the same game shares
/// one map, and a game's map survives being looked away from — rebuilding is
/// not free. Bounded because a busy server can accumulate worlds faster than
/// games end; the eviction is insertion-order rather than true LRU, which for
/// a cache this size costs a rebuild now and then and saves carrying a
/// recency list.
const MAP_CACHE_MAX: usize = 24;
#[derive(Default)]
struct MapCache {
    maps: HashMap<u64, Arc<WorldMap>>,
    /// Insertion order, for the bound above.
    order: VecDeque<u64>,
}

static MAP_CACHE: LazyLock<Mutex<MapCache>> = LazyLock::new(|| Mutex::new(MapCache::default()));

/// The built world for a spec, from cache or freshly generated.
pub fn map_for_spec(spec: &GeneratedMapSpec) -> Arc<WorldMap> {
    let key = spec.key();
    if let Ok(cache) = MAP_CACHE.lock()
        && let Some(found) = cache.maps.get(&key)
    {
        return found.clone();
    }
    // Generated outside the lock: two sessions opening the same new game at
    // the same moment would otherwise queue behind each other for the whole
    // build, and building twice is cheaper than blocking a render.
    let built = Arc::new(generate(spec));
    if let Ok(mut cache) = MAP_CACHE.lock() {
        if cache.maps.insert(key, built.clone()).is_none() {
            cache.order.push_back(key);
        }
        while cache.order.len() > MAP_CACHE_MAX {
            if let Some(oldest) = cache.order.pop_front() {
                cache.maps.remove(&oldest);
            }
        }
    }
    built
}

// ----- generation

/// Build the world a spec describes. Deterministic: same spec, same map, on
/// any machine and any run.
pub fn generate(spec: &GeneratedMapSpec) -> WorldMap {
    let spec = spec.normalized();
    let mut rng = SplitMix64::new(spec.seed ^ 0x5EED_0FA0_0D1E_5001);
    let width = GEN_WIDTH;
    let height = GEN_HEIGHT;
    let cell_area = cell_areas(height);

    let grown: Vec<Vec<usize>> = match spec.generator {
        Generator::Flood => flood_landmasses(&spec, &cell_area, &mut rng),
        Generator::Relief => relief_landmasses(&spec, &cell_area, &mut rng),
    };
    let mut cells = vec![WATER; width as usize * height as usize];
    let counts = allocate_territories(&spec, &grown, &cell_area);

    let mut territories: Vec<Territory> = Vec::with_capacity(spec.territories as usize);
    for (mass_index, mass_cells) in grown.iter().enumerate() {
        let want = counts[mass_index];
        if want == 0 || mass_cells.is_empty() {
            continue;
        }
        let members = match spec.generator {
            Generator::Flood => partition_flood(mass_cells, want, width, &mut rng),
            Generator::Relief => partition_relief(mass_cells, want, &mut rng),
        };
        found_territories(
            &members,
            width,
            &mut cells,
            &mut territories,
            &cell_area,
            &mut rng,
        );
    }

    let names = resolve_names(&spec, territories.len());
    for (territory, (name, iso)) in territories.iter_mut().zip(names) {
        territory.name = name;
        territory.iso = iso;
    }

    let grid = TerritoryGrid {
        width,
        height,
        cells,
    };
    compute_neighbors(&mut territories, &grid);
    compute_landmasses(&mut territories, &grid);
    compute_bboxes(&mut territories, &grid);
    compute_coastal(&mut territories, &grid);
    compute_sea_lanes(&mut territories, &grid);
    compute_weights(&mut territories);
    let mips = build_mips(&grid);

    WorldMap {
        id: "generated".to_string(),
        display_name: "Uncharted".to_string(),
        territories,
        grid,
        mips,
    }
}

/// A landmass before it has any cells. The shape comes from three things
/// working together, because any one of them alone reads as a blob: several
/// nuclei (so a continent has an axis and lobes instead of a middle), a few
/// angular harmonics on the radius (bays and capes at continental scale),
/// and fractal noise warping the whole field (coastline detail at every
/// scale down to the cell).
struct MassPlan {
    /// Where the growth starts. Always one cell, so a landmass is connected
    /// by construction however wild the field gets.
    start: (u16, u16),
    /// The centres the distance field is measured from — the nearest one
    /// wins, which is what turns a disc into something with an axis.
    nuclei: Vec<(f64, f64)>,
    target_area: f64,
    /// Roughly how far the coast should sit from a nucleus, in cells. The
    /// field is normalised by this so the noise means the same thing on a
    /// continent and on an islet.
    radius_cells: f64,
    lobes: [(f64, f64); 4],
    noise: Noise,
    /// Two more noise fields, used to bend the *coordinates* before the shape
    /// is measured rather than the shape itself. Warping the domain is what
    /// turns a wiggly circle into a coastline: gulfs that cut deep, capes
    /// that hook, an isthmus where two lobes nearly miss each other.
    warp_x: Noise,
    warp_y: Noise,
    /// How hard the fractal noise pushes the coastline about.
    roughness: f64,
}

/// How much land each mass is planned to hold, continents first and then
/// islands, and how many of them are continents.
fn planned_areas(spec: &GeneratedMapSpec, rng: &mut SplitMix64) -> (Vec<f64>, usize) {
    let globe_area = 4.0 * std::f64::consts::PI * EARTH_RADIUS_KM * EARTH_RADIUS_KM;
    let land_area = globe_area * LAND_FRACTION;
    let (continent_area, island_area) = match (spec.continents, spec.islands) {
        (0, _) => (0.0, land_area),
        (_, 0) => (land_area, 0.0),
        _ => (
            land_area * CONTINENT_SHARE,
            land_area * (1.0 - CONTINENT_SHARE),
        ),
    };
    let shares = |count: u8, pool: f64, rng: &mut SplitMix64| -> Vec<f64> {
        if count == 0 {
            return Vec::new();
        }
        // Uneven on purpose: continents that are all the same size make every
        // game the same game.
        let weights: Vec<f64> = (0..count).map(|_| 0.55 + rng.next_f64() * 1.1).collect();
        let total: f64 = weights.iter().sum();
        weights.iter().map(|w| pool * w / total).collect()
    };
    let continent_areas = shares(spec.continents, continent_area, rng);
    let island_areas = shares(spec.islands, island_area, rng);
    let continent_count = continent_areas.len();
    let all: Vec<f64> = continent_areas.into_iter().chain(island_areas).collect();
    (all, continent_count)
}

/// The shape of one landmass around `start`: its nuclei, lobes and noise.
fn shape_plan(start: (u16, u16), area: f64, is_island: bool, rng: &mut SplitMix64) -> MassPlan {
    // The radius a disc of this area would have, in cells at this
    // latitude — the yardstick everything else is measured against.
    let lat = 90.0 - (start.1 as f64 + 0.5) / GEN_HEIGHT as f64 * 180.0;
    let cell_km2 = {
        let d_lon = std::f64::consts::TAU / GEN_WIDTH as f64;
        let top = (lat + 90.0 / GEN_HEIGHT as f64).to_radians();
        let bottom = (lat - 90.0 / GEN_HEIGHT as f64).to_radians();
        (EARTH_RADIUS_KM * EARTH_RADIUS_KM * d_lon * (top.sin() - bottom.sin())).max(1.0)
    };
    let radius_cells = (area / cell_km2 / std::f64::consts::PI).sqrt().max(2.0);

    // Continents get two to four nuclei strung along an axis: that is
    // what gives a mass a long side, a narrow waist, and lobes hanging
    // off it rather than one centre and a rim. Islands get one or two.
    let nuclei_count = if is_island {
        1 + (rng.next_u64() % 2) as usize
    } else {
        2 + (rng.next_u64() % 3) as usize
    };
    let axis = rng.next_f64() * std::f64::consts::TAU;
    let mut nuclei = vec![(start.0 as f64, start.1 as f64)];
    for _ in 1..nuclei_count {
        // Along the axis, but not on it: a string of beads that bends.
        let along = (0.35 + rng.next_f64() * 0.75) * radius_cells;
        let wobble = (rng.next_f64() - 0.5) * 1.1;
        let angle = axis + wobble;
        let lat_scale = lat.to_radians().cos().max(0.2);
        nuclei.push((
            start.0 as f64 + angle.cos() * along / lat_scale,
            (start.1 as f64 + angle.sin() * along).clamp(0.0, (GEN_HEIGHT - 1) as f64),
        ));
    }

    let lobes = [
        (
            0.16 + rng.next_f64() * 0.22,
            rng.next_f64() * std::f64::consts::TAU,
        ),
        (
            0.10 + rng.next_f64() * 0.16,
            rng.next_f64() * std::f64::consts::TAU,
        ),
        (
            0.05 + rng.next_f64() * 0.11,
            rng.next_f64() * std::f64::consts::TAU,
        ),
        (
            0.03 + rng.next_f64() * 0.07,
            rng.next_f64() * std::f64::consts::TAU,
        ),
    ];
    MassPlan {
        start,
        nuclei,
        target_area: area,
        radius_cells,
        lobes,
        noise: Noise::new(rng.next_u64()),
        warp_x: Noise::new(rng.next_u64()),
        warp_y: Noise::new(rng.next_u64()),
        // Islands are rougher for their size: an archipelago reads as
        // scattered rock, a continent as a coastline.
        roughness: if is_island { 0.62 } else { 0.5 },
    }
}

// ----- Generator::Flood

/// Every landmass flooded from its own centre, centres as far apart as they
/// go. The cells of each mass, in mass order.
fn flood_landmasses(
    spec: &GeneratedMapSpec,
    cell_area: &[f64],
    rng: &mut SplitMix64,
) -> Vec<Vec<usize>> {
    let (areas, continent_count) = planned_areas(spec, rng);
    let mut plans: Vec<MassPlan> = Vec::new();
    for (index, area) in areas.into_iter().enumerate() {
        let start = pick_center(&plans, rng);
        plans.push(shape_plan(start, area, index >= continent_count, rng));
    }
    // Landmass index + 1 is painted as each grows, so a flood stops at the
    // masses grown before it.
    let mut owner = vec![0u16; GEN_WIDTH as usize * GEN_HEIGHT as usize];
    plans
        .iter()
        .enumerate()
        .map(|(index, plan)| grow_landmass(plan, index as u16 + 1, &mut owner, cell_area, rng))
        .collect()
}

/// Best-of-N sampling: take the candidate furthest from every landmass
/// already placed. Cheap, and it keeps continents from being generated on top
/// of one another without needing a rejection loop that might not terminate.
fn pick_center(placed: &[MassPlan], rng: &mut SplitMix64) -> (u16, u16) {
    const CANDIDATES: usize = 24;
    let mut best = (0u16, 0u16);
    let mut best_score = f64::NEG_INFINITY;
    let lat_span = MAX_LATITUDE / 90.0;
    for _ in 0..CANDIDATES {
        let x = (rng.next_f64() * GEN_WIDTH as f64) as u16 % GEN_WIDTH;
        // Latitudes are drawn in sine space so centres are spread evenly over
        // the sphere rather than piling up near the poles.
        let s = (rng.next_f64() * 2.0 - 1.0) * lat_span;
        let lat = s.asin().to_degrees() / 90.0 * MAX_LATITUDE;
        let y = (((90.0 - lat) / 180.0) * GEN_HEIGHT as f64) as u16;
        let y = y.min(GEN_HEIGHT - 1);
        if placed.is_empty() {
            return (x, y);
        }
        let score = placed
            .iter()
            .map(|p| wrapped_distance((x, y), p.start))
            .fold(f64::INFINITY, f64::min);
        if score > best_score {
            best_score = score;
            best = (x, y);
        }
    }
    best
}

fn wrapped_distance(a: (u16, u16), b: (u16, u16)) -> f64 {
    let dx = (a.0 as f64 - b.0 as f64).abs();
    let dx = dx.min(GEN_WIDTH as f64 - dx);
    let dy = a.1 as f64 - b.1 as f64;
    (dx * dx + dy * dy).sqrt()
}

/// Flood outward from the centre, cheapest cell first, until the landmass has
/// the area it was planned for. The cost of a cell is its distance from the
/// centre divided by a direction-dependent radius, so a mass with strong
/// lobes grows arms and inlets instead of a circle.
fn grow_landmass(
    plan: &MassPlan,
    mark: u16,
    owner: &mut [u16],
    cell_area: &[f64],
    rng: &mut SplitMix64,
) -> Vec<usize> {
    let width = GEN_WIDTH as i32;
    let height = GEN_HEIGHT as i32;
    let index_of = |x: i32, y: i32| (y * width + x) as usize;
    let start = index_of(plan.start.0 as i32, plan.start.1 as i32);
    if owner[start] != 0 {
        return Vec::new();
    }

    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
    let mut filled: Vec<usize> = Vec::new();
    let mut area = 0.0;
    heap.push(Reverse((0, start)));
    let mut queued = vec![false; owner.len()];
    queued[start] = true;

    while let Some(Reverse((_, index))) = heap.pop() {
        if owner[index] != 0 {
            continue;
        }
        owner[index] = mark;
        filled.push(index);
        area += cell_area[index / GEN_WIDTH as usize];
        if area >= plan.target_area {
            break;
        }
        let x = (index % GEN_WIDTH as usize) as i32;
        let y = (index / GEN_WIDTH as usize) as i32;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = (x + dx).rem_euclid(width);
            let ny = y + dy;
            // The poles are the edge of the world: nothing grows past them.
            if ny < 0 || ny >= height {
                continue;
            }
            let lat = 90.0 - (ny as f64 + 0.5) / GEN_HEIGHT as f64 * 180.0;
            if lat.abs() > MAX_LATITUDE {
                continue;
            }
            let neighbour = index_of(nx, ny);
            if queued[neighbour] || owner[neighbour] != 0 {
                continue;
            }
            queued[neighbour] = true;
            heap.push(Reverse((
                cost_from_center(plan, nx, ny, rng.next_f64()),
                neighbour,
            )));
        }
    }
    filled
}

/// Where this cell sits in the landmass's shape field, as a sortable integer
/// (the heap needs `Ord`, and floats do not have it). Everything below one is
/// inside the coast a disc of the same area would have; the flood fills in
/// field order until it has the area it was planned for, so the field *is*
/// the shape.
///
/// Three layers: distance to the nearest nucleus (an axis rather than a
/// middle), divided by a radius that breathes with the angle (bays and
/// headlands), all warped by fractal noise (everything smaller, down to the
/// ragged cell-scale edge that makes a coast look drawn rather than
/// compassed).
fn cost_from_center(plan: &MassPlan, x: i32, y: i32, jitter: f64) -> u64 {
    // Longitude degrees shrink toward the poles; without this a northern
    // landmass smears sideways across a quarter of the map.
    let lat = 90.0 - (y as f64 + 0.5) / GEN_HEIGHT as f64 * 180.0;
    let lat_scale = lat.to_radians().cos().max(0.2);

    // Bend the ground before measuring it. The sample point is pushed about
    // by two low-frequency fields, so a coast that would have been a smooth
    // arc comes back folded — bays, spits, and the occasional near-miss that
    // leaves an isthmus.
    let u0 = x as f64 / GEN_WIDTH as f64;
    let v0 = y as f64 / GEN_HEIGHT as f64;
    let sway = plan.radius_cells * 0.55;
    let x = x as f64 + plan.warp_x.fbm(u0, v0, 3) * sway;
    let y = y as f64 + plan.warp_y.fbm(u0, v0, 3) * sway;

    let mut nearest = f64::INFINITY;
    for (cx, cy) in &plan.nuclei {
        let dx = {
            let raw = (x - cx).abs();
            let wrapped = GEN_WIDTH as f64 - raw;
            let shortest = raw.min(wrapped);
            if wrapped < raw { -shortest } else { shortest }
        } * lat_scale;
        let dy = y - cy;
        let distance = (dx * dx + dy * dy).sqrt();
        let angle = dy.atan2(dx);
        let mut radius = 1.0;
        for (index, (amplitude, phase)) in plan.lobes.iter().enumerate() {
            // Harmonics 2..5: one lobe is an egg, several are a coastline.
            let harmonic = index as f64 + 2.0;
            radius += amplitude * (angle * harmonic + phase).sin();
        }
        nearest = nearest.min(distance / radius.max(0.3));
    }
    let normalised = nearest / plan.radius_cells;

    // Fractal noise in map space, so neighbouring cells agree and the
    // coastline wanders instead of fizzing. Sampled at the cell, seamless
    // across the antimeridian.
    let warp = plan.noise.fbm(u0, v0, 7);
    let cost = normalised * (1.0 + plan.roughness * warp) + plan.roughness * warp * 0.12;

    // A last flick of per-cell jitter, so the very edge is ragged at the
    // scale of a single cell rather than smooth between noise lattice points.
    let cost = cost * (1.0 + jitter * 0.05);
    (cost.max(0.0) * 4096.0) as u64
}

/// Value noise on a lattice, summed over octaves. Deterministic from a seed,
/// seamless east-west (the lattice wraps with the world), and cheap: a
/// generated world evaluates this a few hundred thousand times.
struct Noise {
    seed: u64,
}

impl Noise {
    fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// One lattice point's value in [-1, 1]. `period` is how many lattice
    /// cells span the world at this octave, so wrapping x by it keeps the
    /// antimeridian from showing as a seam.
    fn lattice(&self, x: i64, y: i64, period: i64) -> f64 {
        let x = x.rem_euclid(period.max(1));
        let mut h = self
            .seed
            .wrapping_add((x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_add((y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        ((h >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }

    /// Smoothed value noise at (u, v) in unit map space, `freq` lattice cells
    /// across the world.
    fn at(&self, u: f64, v: f64, freq: i64) -> f64 {
        let x = u * freq as f64;
        let y = v * freq as f64;
        let (xi, yi) = (x.floor(), y.floor());
        let (fx, fy) = (x - xi, y - yi);
        // Smoothstep, so the lattice does not show as a grid of creases.
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let (xi, yi) = (xi as i64, yi as i64);
        let v00 = self.lattice(xi, yi, freq);
        let v10 = self.lattice(xi + 1, yi, freq);
        let v01 = self.lattice(xi, yi + 1, freq);
        let v11 = self.lattice(xi + 1, yi + 1, freq);
        let top = v00 + (v10 - v00) * sx;
        let bottom = v01 + (v11 - v01) * sx;
        top + (bottom - top) * sy
    }

    /// Octaves of `at`, each twice as fine and half as loud — the standard
    /// recipe, and the reason a coastline has both bays and inlets.
    fn fbm(&self, u: f64, v: f64, octaves: u32) -> f64 {
        self.fbm_from(u, v, octaves, 6)
    }

    /// `fbm` starting at `base` lattice cells across the world.
    fn fbm_from(&self, u: f64, v: f64, octaves: u32, base: i64) -> f64 {
        let mut total = 0.0;
        let mut amplitude = 1.0;
        let mut freq = base;
        let mut norm = 0.0;
        for _ in 0..octaves {
            total += self.at(u, v, freq) * amplitude;
            norm += amplitude;
            amplitude *= 0.5;
            freq *= 2;
        }
        total / norm.max(f64::EPSILON)
    }
}

/// How many territories each landmass carries, proportional to the area it
/// actually got (not the area it was planned for — a mass that ran into its
/// neighbours is smaller than intended and should hold fewer countries).
fn allocate_territories(
    spec: &GeneratedMapSpec,
    grown: &[Vec<usize>],
    cell_area: &[f64],
) -> Vec<usize> {
    let areas: Vec<f64> = grown
        .iter()
        .map(|cells| {
            cells
                .iter()
                .map(|index| cell_area[index / GEN_WIDTH as usize])
                .sum()
        })
        .collect();
    let total: f64 = areas.iter().sum();
    let want = spec.territories as usize;
    if total <= 0.0 {
        return vec![0; grown.len()];
    }

    let mut counts: Vec<usize> = areas
        .iter()
        .zip(grown)
        .map(|(area, cells)| {
            if cells.is_empty() {
                return 0;
            }
            // Every landmass that exists at all holds at least one country,
            // and none can hold more countries than it has cells.
            ((area / total * want as f64).round() as usize)
                .max(1)
                .min(cells.len())
        })
        .collect();

    // Rounding rarely lands on the exact total; push the difference onto (or
    // off) the biggest landmasses, which can absorb it without any of their
    // countries turning into specks.
    let order: Vec<usize> = {
        let mut order: Vec<usize> = (0..grown.len()).collect();
        order.sort_by(|a, b| areas[*b].total_cmp(&areas[*a]));
        order
    };
    let mut guard = 0;
    while counts.iter().sum::<usize>() != want && guard < want * 4 + 64 {
        guard += 1;
        let have: usize = counts.iter().sum();
        if have < want {
            for index in &order {
                if counts[*index] < grown[*index].len() {
                    counts[*index] += 1;
                    break;
                }
            }
        } else {
            for index in order.iter().rev() {
                if counts[*index] > 1 {
                    counts[*index] -= 1;
                    break;
                }
            }
        }
    }
    counts
}

/// Cut one landmass into `want` territories: scatter seeds far apart, then
/// let them race outward at different speeds. The speed spread is what gives
/// a world a few sprawling countries and a lot of small ones, which is how
/// the real one looks and what makes the map worth reading. Four-way, so
/// the countries come out as diamonds. The cells of each, by seed.
fn partition_flood(
    mass_cells: &[usize],
    want: usize,
    width: u16,
    rng: &mut SplitMix64,
) -> Vec<Vec<usize>> {
    let seeds = scatter_seeds(mass_cells, want, rng);
    let speeds: Vec<f64> = (0..seeds.len())
        .map(|_| {
            // Log-normal-ish: most countries middling, a few much bigger.
            let u = rng.next_f64();
            (0.45 + u * u * 2.6).max(0.2)
        })
        .collect();

    // Assignment lives in a flat array rather than a map: a `HashMap` walked
    // in its own order would hand the cells to each territory in an order
    // that changes between processes, and summing areas in a different order
    // changes the last bit of a float — which is enough to make the same seed
    // build two different worlds.
    let grid_cells = GEN_WIDTH as usize * GEN_HEIGHT as usize;
    let mut assigned: Vec<u16> = vec![UNASSIGNED; grid_cells];
    let mut in_mass: Vec<bool> = vec![false; grid_cells];
    for index in mass_cells {
        in_mass[*index] = true;
    }
    let mut heap: BinaryHeap<Reverse<(u64, usize, u16)>> = BinaryHeap::new();
    for (slot, cell) in seeds.iter().enumerate() {
        heap.push(Reverse((0, *cell, slot as u16)));
    }
    while let Some(Reverse((cost, index, slot))) = heap.pop() {
        if assigned[index] != UNASSIGNED {
            continue;
        }
        assigned[index] = slot;
        let x = (index % width as usize) as i32;
        let y = (index / width as usize) as i32;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = (x + dx).rem_euclid(width as i32);
            let ny = y + dy;
            if ny < 0 || ny >= GEN_HEIGHT as i32 {
                continue;
            }
            let neighbour = (ny * width as i32 + nx) as usize;
            if !in_mass[neighbour] || assigned[neighbour] != UNASSIGNED {
                continue;
            }
            let step = (72.0 / speeds[slot as usize] * (0.75 + rng.next_f64() * 0.5)) as u64;
            heap.push(Reverse((cost + step.max(1), neighbour, slot)));
        }
    }
    members_by_slot(mass_cells, &assigned, seeds.len())
}

const UNASSIGNED: u16 = u16::MAX;

/// Each slot's cells, in the landmass's own cell order.
fn members_by_slot(mass_cells: &[usize], assigned: &[u16], slots: usize) -> Vec<Vec<usize>> {
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); slots];
    for index in mass_cells {
        let slot = assigned[*index];
        if slot != UNASSIGNED {
            members[slot as usize].push(*index);
        }
    }
    members
}

/// Paint one landmass's countries onto the grid and give each its numbers.
fn found_territories(
    members: &[Vec<usize>],
    width: u16,
    cells: &mut [u16],
    territories: &mut Vec<Territory>,
    cell_area: &[f64],
    rng: &mut SplitMix64,
) {
    let first_id = territories.len() as TerritoryId;
    for (slot, member_cells) in members.iter().enumerate() {
        let id = first_id + slot as TerritoryId;
        for index in member_cells {
            cells[*index] = id;
        }
        let area_km2: f64 = member_cells
            .iter()
            .map(|index| cell_area[index / width as usize])
            .sum();
        // Density spread over three orders of magnitude, like the real world:
        // empty interiors and crowded coasts, without either being the rule.
        let u = rng.next_f64();
        let density = 2.0 * (400.0f64).powf(u * u);
        let population = (area_km2 * density).max(1_200.0) as u64;
        let center = center_cell(member_cells, width);
        territories.push(Territory {
            id,
            iso: String::new(),
            name: String::new(),
            population,
            area_km2: area_km2.max(1.0) as u64,
            neighbors: Vec::new(),
            landmasses: Vec::new(),
            land_links: Vec::new(),
            sea_lanes: Vec::new(),
            center,
            coastal: false,
            bbox: (center.0, center.1, center.0, center.1),
            weight: 0.0,
        });
    }
}

// ----- Generator::Relief

/// How far past a mass's own radius its rise is evaluated: past it the
/// field is far below any sea level the land fraction sets.
const RELIEF_REACH: f64 = 3.2;
/// Land pieces smaller than this many cells (~25,000 km² at the equator)
/// go back under the sea: every landmass holds at least one country, and a
/// speck is a seat in the standings nobody can see.
const RELIEF_MIN_CELLS: usize = 16;
/// Where the land starts to sink towards the poles, in degrees: the coast
/// curves away before `MAX_LATITUDE` instead of being cut off along it.
const RELIEF_POLAR_FALL: f64 = 50.0;

/// Every landmass a rise in one elevation field, cut at the sea level that
/// leaves Earth's share of land. Continents are strung off one another and
/// islands sit off their coasts, so no shore is an ocean away from all the
/// rest; where two rises meet they merge into one coast, and where the field
/// hovers near the sea level it leaves capes, straits and offshore islets.
/// The cells of each landmass, in grid order.
fn relief_landmasses(
    spec: &GeneratedMapSpec,
    cell_area: &[f64],
    rng: &mut SplitMix64,
) -> Vec<Vec<usize>> {
    let (areas, continent_count) = planned_areas(spec, rng);
    let mut plans: Vec<MassPlan> = Vec::new();
    for (index, area) in areas.iter().enumerate() {
        let is_island = index >= continent_count;
        let start = relief_center(&plans, *area, is_island, continent_count, rng);
        plans.push(shape_plan(start, *area, is_island, rng));
    }
    let detail = Noise::new(rng.next_u64());

    let width = GEN_WIDTH as i32;
    let height = GEN_HEIGHT as i32;
    let mut elevation = vec![f64::NEG_INFINITY; width as usize * height as usize];
    for plan in &plans {
        let lat = cell_latitude(plan.start.1 as i32);
        let lat_scale = lat.to_radians().cos().max(0.2);
        let reach_y = (plan.radius_cells * RELIEF_REACH) as i32 + 1;
        let reach_x = ((plan.radius_cells * RELIEF_REACH / lat_scale) as i32 + 1).min(width / 2);
        for dy in -reach_y..=reach_y {
            let y = plan.start.1 as i32 + dy;
            if y < 0 || y >= height || cell_latitude(y).abs() > MAX_LATITUDE {
                continue;
            }
            for dx in -reach_x..=reach_x {
                let x = (plan.start.0 as i32 + dx).rem_euclid(width);
                let rise = 1.0 - cost_from_center(plan, x, y, 0.0) as f64 / 4096.0;
                let slot = &mut elevation[(y * width + x) as usize];
                *slot = slot.max(rise);
            }
        }
    }
    for (index, value) in elevation.iter_mut().enumerate() {
        if !value.is_finite() {
            continue;
        }
        let x = (index % width as usize) as f64;
        let y = index / width as usize;
        let (u, v) = (x / GEN_WIDTH as f64, y as f64 / GEN_HEIGHT as f64);
        let polar = ((cell_latitude(y as i32).abs() - RELIEF_POLAR_FALL)
            / (MAX_LATITUDE - RELIEF_POLAR_FALL))
            .max(0.0);
        *value += 0.18 * detail.fbm_from(u, v, 5, 12) - 1.4 * polar * polar;
    }

    // The sea level: the highest ground first, until the land is the area
    // the spec planned. Ties go by cell, so the cut is the same every time.
    let mut ground: Vec<usize> = (0..elevation.len())
        .filter(|index| elevation[*index].is_finite())
        .collect();
    ground.sort_by(|a, b| elevation[*b].total_cmp(&elevation[*a]).then(a.cmp(b)));
    let target: f64 = areas.iter().sum();
    let mut land = vec![false; elevation.len()];
    let mut area = 0.0;
    for index in ground {
        if area >= target {
            break;
        }
        land[index] = true;
        area += cell_area[index / GEN_WIDTH as usize];
    }

    // The landmasses are what the sea level left joined, whichever rise
    // each piece came from. A spec with no islands keeps only the pieces a
    // continent rose from: the islets the detail leaves offshore are islands.
    let starts: Vec<usize> = plans
        .iter()
        .map(|p| p.start.1 as usize * GEN_WIDTH as usize + p.start.0 as usize)
        .collect();
    let mut seen = vec![false; land.len()];
    let mut masses: Vec<Vec<usize>> = Vec::new();
    for first in 0..land.len() {
        if !land[first] || seen[first] {
            continue;
        }
        seen[first] = true;
        let mut mass = vec![first];
        let mut cursor = 0;
        while cursor < mass.len() {
            let index = mass[cursor];
            cursor += 1;
            let x = (index % width as usize) as i32;
            let y = (index / width as usize) as i32;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = ((x + dx).rem_euclid(width), y + dy);
                if ny < 0 || ny >= height {
                    continue;
                }
                let next = (ny * width + nx) as usize;
                if land[next] && !seen[next] {
                    seen[next] = true;
                    mass.push(next);
                }
            }
        }
        mass.sort_unstable();
        let risen = starts.iter().any(|start| mass.binary_search(start).is_ok());
        if mass.len() >= RELIEF_MIN_CELLS && (spec.islands > 0 || risen) {
            masses.push(mass);
        }
    }
    masses
}

/// Where a relief mass rises. The first anywhere in the temperate band;
/// each continent after it off the coast of one already placed, so the
/// continents hang together as a world rather than as scattered discs; each
/// island off a continent's shore. Best of a few draws, by how clear it
/// keeps of every mass already placed (overlap merges coasts, which is
/// fine in moderation and a blob in excess).
fn relief_center(
    placed: &[MassPlan],
    area: f64,
    is_island: bool,
    continent_count: usize,
    rng: &mut SplitMix64,
) -> (u16, u16) {
    const CANDIDATES: usize = 24;
    const MAX_CENTER_LAT: f64 = 56.0;
    let radius_km = |area: f64| (area / std::f64::consts::PI).sqrt();
    let to_cell = |(lat, lon): (f64, f64)| -> (u16, u16) {
        let x = ((lon + 180.0) / 360.0 * GEN_WIDTH as f64).floor() as i64;
        let y = ((90.0 - lat) / 180.0 * GEN_HEIGHT as f64).floor() as i64;
        (
            x.rem_euclid(GEN_WIDTH as i64) as u16,
            y.clamp(0, GEN_HEIGHT as i64 - 1) as u16,
        )
    };
    if placed.is_empty() {
        let lat = ((rng.next_f64() * 2.0 - 1.0) * 40f64.to_radians().sin())
            .asin()
            .to_degrees();
        let lon = rng.next_f64() * 360.0 - 180.0;
        return to_cell((lat, lon));
    }
    let parents = match is_island && continent_count > 0 {
        true => &placed[..continent_count.min(placed.len())],
        false => placed,
    };
    let own = radius_km(area);
    let mut best = parents[0].start;
    let mut best_score = f64::NEG_INFINITY;
    for _ in 0..CANDIDATES {
        let parent = &parents[(rng.next_u64() % parents.len() as u64) as usize];
        let parent_radius = radius_km(parent.target_area);
        let (spread, own_spread, bearing) = (rng.next_f64(), rng.next_f64(), rng.next_f64());
        let gap = match is_island {
            true => parent_radius * (1.0 + spread * 0.35) + own * (1.2 + own_spread),
            false => (parent_radius + own) * (0.85 + spread * 0.4),
        };
        let (lat, lon) = travel(
            cell_lat_lon(parent.start),
            bearing * std::f64::consts::TAU,
            gap,
        );
        if lat.abs() > MAX_CENTER_LAT {
            continue;
        }
        let candidate = to_cell((lat, lon));
        let clear = placed
            .iter()
            .map(|p| {
                great_circle_km(cell_lat_lon(p.start), cell_lat_lon(candidate))
                    / (radius_km(p.target_area) + own)
            })
            .fold(f64::INFINITY, f64::min);
        // Past clear of everything is no better: farther only opens an ocean.
        let score = clear.min(1.0);
        if score > best_score {
            best_score = score;
            best = candidate;
        }
    }
    best
}

/// Latitude of a grid row's centre, in degrees.
fn cell_latitude(y: i32) -> f64 {
    90.0 - (y as f64 + 0.5) / GEN_HEIGHT as f64 * 180.0
}

fn cell_lat_lon((x, y): (u16, u16)) -> (f64, f64) {
    (
        cell_latitude(y as i32),
        (x as f64 + 0.5) / GEN_WIDTH as f64 * 360.0 - 180.0,
    )
}

fn great_circle_km((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().clamp(0.0, 1.0).asin()
}

/// Where `km` along `bearing` (radians, clockwise from north) from a point
/// lands, on the great circle.
fn travel((lat, lon): (f64, f64), bearing: f64, km: f64) -> (f64, f64) {
    let delta = km / EARTH_RADIUS_KM;
    let p1 = lat.to_radians();
    let p2 = (p1.sin() * delta.cos() + p1.cos() * delta.sin() * bearing.cos()).asin();
    let l2 = lon.to_radians()
        + (bearing.sin() * delta.sin() * p1.cos()).atan2(delta.cos() - p1.sin() * p2.sin());
    let lon = (l2.to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
    (p2.to_degrees(), lon)
}

/// Cut one landmass into `want` territories over rough ground: seeds race
/// outward eight ways, slowed by a noise field, so borders bend where the
/// ground does instead of running as straight diamonds. Sizes vary by where
/// the seeds fall (`scatter_seeds_spaced`), not by how fast they grow: a
/// slow seed boxed in by fast ones comes out as an enclave, a country
/// sitting in another like a hole. The cells of each, by seed.
fn partition_relief(mass_cells: &[usize], want: usize, rng: &mut SplitMix64) -> Vec<Vec<usize>> {
    let spacing = Noise::new(rng.next_u64());
    let seeds = scatter_seeds_spaced(mass_cells, want, &spacing, rng);
    let speeds: Vec<f64> = (0..seeds.len())
        .map(|_| 0.8 + rng.next_f64() * 0.5)
        .collect();
    let ground = Noise::new(rng.next_u64());
    let width = GEN_WIDTH as i32;
    let grid_cells = GEN_WIDTH as usize * GEN_HEIGHT as usize;
    let mut assigned: Vec<u16> = vec![UNASSIGNED; grid_cells];
    let mut in_mass: Vec<bool> = vec![false; grid_cells];
    for index in mass_cells {
        in_mass[*index] = true;
    }
    // How hard a cell is to cross: rough where the noise runs far from
    // zero, easy along its zero lines, which become the valleys countries
    // spread down and the ridges they stop at.
    let roughness = |index: usize| -> f64 {
        let u = (index % GEN_WIDTH as usize) as f64 / GEN_WIDTH as f64;
        let v = (index / GEN_WIDTH as usize) as f64 / GEN_HEIGHT as f64;
        1.0 + 2.2 * ground.fbm_from(u, v, 4, 24).abs()
    };
    let mut heap: BinaryHeap<Reverse<(u64, usize, u16)>> = BinaryHeap::new();
    for (slot, cell) in seeds.iter().enumerate() {
        heap.push(Reverse((0, *cell, slot as u16)));
    }
    while let Some(Reverse((cost, index, slot))) = heap.pop() {
        if assigned[index] != UNASSIGNED {
            continue;
        }
        assigned[index] = slot;
        let x = (index % GEN_WIDTH as usize) as i32;
        let y = (index / GEN_WIDTH as usize) as i32;
        // A step east or west is shorter on the ground the further from
        // the equator, as the cells are.
        let lat_scale = cell_latitude(y).to_radians().cos().max(0.2);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let ny = y + dy;
                if (dx, dy) == (0, 0) || ny < 0 || ny >= GEN_HEIGHT as i32 {
                    continue;
                }
                let neighbour = (ny * width + (x + dx).rem_euclid(width)) as usize;
                if !in_mass[neighbour] || assigned[neighbour] != UNASSIGNED {
                    continue;
                }
                let run = ((dx as f64 * lat_scale).powi(2) + (dy as f64).powi(2)).sqrt();
                let step = (100.0 * run * roughness(neighbour) / speeds[slot as usize]) as u64;
                heap.push(Reverse((cost + step.max(1), neighbour, slot)));
            }
        }
    }
    members_by_slot(mass_cells, &assigned, seeds.len())
}

/// Farthest-point sampling, approximated: each new seed is the best of a
/// bounded sample rather than the true maximum. Exact sampling is O(k·n) and
/// n is a hundred thousand cells; this is O(k·samples) and the difference is
/// invisible on a map.
fn scatter_seeds(mass_cells: &[usize], want: usize, rng: &mut SplitMix64) -> Vec<usize> {
    const SAMPLES: usize = 256;
    let want = want.min(mass_cells.len()).max(1);
    let mut seeds: Vec<usize> = Vec::with_capacity(want);
    seeds.push(mass_cells[(rng.next_u64() % mass_cells.len() as u64) as usize]);
    let coords = |index: usize| -> (u16, u16) {
        (
            (index % GEN_WIDTH as usize) as u16,
            (index / GEN_WIDTH as usize) as u16,
        )
    };
    while seeds.len() < want {
        let mut best = mass_cells[0];
        let mut best_score = -1.0;
        for _ in 0..SAMPLES {
            let candidate = mass_cells[(rng.next_u64() % mass_cells.len() as u64) as usize];
            if seeds.contains(&candidate) {
                continue;
            }
            let point = coords(candidate);
            let score = seeds
                .iter()
                .map(|s| wrapped_distance(point, coords(*s)))
                .fold(f64::INFINITY, f64::min);
            if score > best_score {
                best_score = score;
                best = candidate;
            }
        }
        if seeds.contains(&best) {
            // Every sample collided with an existing seed: the landmass is
            // saturated, and asking for more countries than it has room for
            // would spin here forever.
            break;
        }
        seeds.push(best);
    }
    seeds
}

/// `scatter_seeds` over uneven ground: a slow noise field sets how far apart
/// seeds want to be, so one region fills with small countries and the next
/// holds a few sprawling ones, which is how the real world reads (crowded
/// coasts, empty interiors) without any one of them swallowing another.
fn scatter_seeds_spaced(
    mass_cells: &[usize],
    want: usize,
    spacing: &Noise,
    rng: &mut SplitMix64,
) -> Vec<usize> {
    const SAMPLES: usize = 256;
    let want = want.min(mass_cells.len()).max(1);
    let coords = |index: usize| -> (u16, u16) {
        (
            (index % GEN_WIDTH as usize) as u16,
            (index / GEN_WIDTH as usize) as u16,
        )
    };
    // From about half to about twice the even spacing.
    let room = |index: usize| -> f64 {
        let (x, y) = coords(index);
        let (u, v) = (x as f64 / GEN_WIDTH as f64, y as f64 / GEN_HEIGHT as f64);
        2f64.powf(1.6 * spacing.fbm_from(u, v, 3, 8))
    };
    let mut seeds: Vec<usize> = Vec::with_capacity(want);
    seeds.push(mass_cells[(rng.next_u64() % mass_cells.len() as u64) as usize]);
    while seeds.len() < want {
        let mut best = mass_cells[0];
        let mut best_score = -1.0;
        for _ in 0..SAMPLES {
            let candidate = mass_cells[(rng.next_u64() % mass_cells.len() as u64) as usize];
            if seeds.contains(&candidate) {
                continue;
            }
            let point = coords(candidate);
            let clear = seeds
                .iter()
                .map(|s| wrapped_distance(point, coords(*s)))
                .fold(f64::INFINITY, f64::min);
            let score = clear / room(candidate);
            if score > best_score {
                best_score = score;
                best = candidate;
            }
        }
        if seeds.contains(&best) {
            // Saturated: see `scatter_seeds`.
            break;
        }
        seeds.push(best);
    }
    seeds
}

/// A cell of the territory nearest its own centroid — the label anchor and
/// the cursor's landing spot, so it has to be *inside* the country rather
/// than at the middle of a crescent.
fn center_cell(member_cells: &[usize], width: u16) -> (u16, u16) {
    let n = member_cells.len().max(1) as f64;
    let (mut sx, mut sy) = (0.0, 0.0);
    for index in member_cells {
        sx += (index % width as usize) as f64;
        sy += (index / width as usize) as f64;
    }
    let (cx, cy) = (sx / n, sy / n);
    let mut best = member_cells[0];
    let mut best_d = f64::INFINITY;
    for index in member_cells {
        let x = (index % width as usize) as f64;
        let y = (index / width as usize) as f64;
        let d = (x - cx).powi(2) + (y - cy).powi(2);
        if d < best_d {
            best_d = d;
            best = *index;
        }
    }
    (
        (best % width as usize) as u16,
        (best / width as usize) as u16,
    )
}

/// Land adjacency straight off the grid, wrapping east-west because the
/// world does. Symmetric by construction.
fn compute_neighbors(territories: &mut [Territory], grid: &TerritoryGrid) {
    let width = grid.width as i32;
    let height = grid.height as i32;
    let mut sets: Vec<std::collections::BTreeSet<TerritoryId>> =
        vec![Default::default(); territories.len()];
    for y in 0..height {
        for x in 0..width {
            let cell = grid.cells[(y * width + x) as usize];
            if cell == WATER {
                continue;
            }
            // East and south only: every pair is seen once and recorded both
            // ways, which is cheaper than four looks and symmetric for free.
            for (dx, dy) in [(1, 0), (0, 1)] {
                let nx = (x + dx).rem_euclid(width);
                let ny = y + dy;
                if ny >= height {
                    continue;
                }
                let other = grid.cells[(ny * width + nx) as usize];
                if other == WATER || other == cell {
                    continue;
                }
                if let Some(set) = sets.get_mut(cell as usize) {
                    set.insert(other);
                }
                if let Some(set) = sets.get_mut(other as usize) {
                    set.insert(cell);
                }
            }
        }
    }
    for (territory, set) in territories.iter_mut().zip(sets) {
        territory.neighbors = set.into_iter().collect();
    }
}

// ----- names

const NAME_POOL_RAW: &str = include_str!("../../../../assets/realm/invented_names.txt");

/// The built-in pool: a thousand invented country names, used when the AI
/// service is off, out of budget, or too slow, and always in tests. A world
/// named from the pool is not a worse world — it is the same world with a
/// different atlas.
pub static NAME_POOL: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    NAME_POOL_RAW
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
});

/// `count` distinct names drawn from the pool, deterministically for a seed.
/// Asking for more than the pool holds (it holds a thousand; a map holds at
/// most four hundred) would repeat, so it does not happen.
pub fn pool_names(seed: u64, count: usize) -> Vec<String> {
    let pool = &*NAME_POOL;
    let mut rng = SplitMix64::new(seed ^ 0xabcd_ef01_2345_6789);
    let mut indices: Vec<usize> = (0..pool.len()).collect();
    // Fisher-Yates, so a world never repeats a name and the order is not the
    // pool's order (which is alphabetical, and would put every world's names
    // in the same family).
    for i in (1..indices.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        indices.swap(i, j);
    }
    indices
        .into_iter()
        .take(count)
        .map(|i| pool[i].to_string())
        .collect()
}

/// Names and ISO-ish codes in territory id order: the spec's names first
/// (they may have come from the AI service), topped up from the pool if it is
/// short, and de-duplicated because two countries with one name is a bug the
/// player sees.
fn resolve_names(spec: &GeneratedMapSpec, count: usize) -> Vec<(String, String)> {
    let mut names: Vec<String> = Vec::with_capacity(count);
    let mut seen: std::collections::HashSet<String> = Default::default();
    for name in spec.names.iter() {
        let trimmed = name.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_lowercase()) {
            continue;
        }
        names.push(trimmed.to_string());
        if names.len() == count {
            break;
        }
    }
    if names.len() < count {
        for name in pool_names(spec.seed, NAME_POOL.len()) {
            if names.len() == count {
                break;
            }
            if seen.insert(name.to_lowercase()) {
                names.push(name);
            }
        }
    }
    // Still short only if the pool itself ran out, which needs a map bigger
    // than the generator allows; numbered names beat empty ones.
    while names.len() < count {
        names.push(format!("Territory {}", names.len() + 1));
    }

    let mut codes: std::collections::HashSet<String> = Default::default();
    names
        .into_iter()
        .map(|name| {
            let code = unique_code(&name, &mut codes);
            (name, code)
        })
        .collect()
}

/// A three-letter code for a name, unique within the map. Earth's territories
/// carry real ISO codes and the UI shows them, so generated ones need
/// something in the same shape.
fn unique_code(name: &str, taken: &mut std::collections::HashSet<String>) -> String {
    let letters: Vec<char> = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let mut candidates: Vec<String> = Vec::new();
    if letters.len() >= 3 {
        candidates.push(letters[..3].iter().collect());
        candidates.push(
            [letters[0], letters[1], letters[letters.len() - 1]]
                .iter()
                .collect(),
        );
        candidates.push(
            [letters[0], letters[2], letters[letters.len() - 1]]
                .iter()
                .collect(),
        );
    }
    for candidate in candidates {
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    let base: String = letters.iter().take(2).collect();
    for n in 0..1000 {
        let candidate = format!("{base}{n}");
        let candidate: String = candidate.chars().take(3).collect();
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    "ZZZ".to_string()
}

// ----- helpers

/// Area of one grid cell per row, in km². An equirectangular row shrinks with
/// the cosine of its latitude, so counting cells would hand a polar country
/// the area of a tropical one; this is what makes "area" mean something the
/// same way it does on Earth.
fn cell_areas(height: u16) -> Vec<f64> {
    let d_lon = std::f64::consts::TAU / GEN_WIDTH as f64;
    (0..height)
        .map(|y| {
            let top = (90.0 - y as f64 / height as f64 * 180.0).to_radians();
            let bottom = (90.0 - (y as f64 + 1.0) / height as f64 * 180.0).to_radians();
            EARTH_RADIUS_KM * EARTH_RADIUS_KM * d_lon * (top.sin() - bottom.sin())
        })
        .collect()
}

/// The same generator the resolver uses for day seeds: portable, tiny, and
/// identical on every platform, which is the whole requirement for a world
/// that has to rebuild the same way in a year.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
#[path = "mapgen_test.rs"]
mod mapgen_test;
