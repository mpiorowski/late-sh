//! realm-mapgen: Natural Earth admin-0 countries GeoJSON -> the two committed
//! realm map assets:
//!   - earth_territories.json: id, iso, name, population, area_km2,
//!     neighbors, center (grid coords)
//!   - earth_grid.bin: equirectangular RLE cell grid, u16 territory id per
//!     cell, 0xFFFF water (format doc in late-ssh lobby/realm/map.rs)
//!
//! Input is downloaded by the developer (public domain), e.g.:
//!   curl -sL -o tmp/realm/ne_50m.geojson \
//!     https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_50m_admin_0_countries.geojson
//! Run:
//!   cargo run -p realm-mapgen --release -- \
//!     --input tmp/realm/ne_50m.geojson --out late-ssh/assets/realm
//!   # finer map (~5km/cell, roughly 4x the asset size):
//!   cargo run -p realm-mapgen --release -- \
//!     --input tmp/realm/ne_50m.geojson --out late-ssh/assets/realm --width 8192
//!   cargo run -p realm-mapgen --release -- --check --out late-ssh/assets/realm

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// Default grid: ~10km per cell at the equator. `--width` doubles or halves
/// it (height is always half the width — the grid is equirectangular over
/// the whole globe). The asset header carries w/h, and the game's reader
/// takes them from there, so a regenerated map at another resolution needs
/// no code change on the other side.
const DEFAULT_WIDTH: usize = 4096;

/// Grid resolution for one run.
#[derive(Clone, Copy)]
struct Res {
    w: usize,
    h: usize,
}

impl Res {
    fn new(w: usize) -> Result<Self> {
        if !w.is_power_of_two() || !(1024..=16384).contains(&w) {
            bail!("--width must be a power of two between 1024 and 16384");
        }
        Ok(Self { w, h: w / 2 })
    }
}
const WATER: u16 = u16::MAX;
const MAGIC: &[u8; 4] = b"RLM1";
const EARTH_RADIUS_KM: f64 = 6371.0;

/// A polygon ring as (lon, lat) vertices.
type Ring = Vec<(f64, f64)>;
/// One polygon: outer ring plus holes.
type Polygon = (Ring, Vec<Ring>);

struct Country {
    iso: String,
    name: String,
    population: u64,
    polygons: Vec<Polygon>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut input: Option<PathBuf> = None;
    let mut out = PathBuf::from("late-ssh/assets/realm");
    let mut check = false;
    let mut width = DEFAULT_WIDTH;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                input = Some(PathBuf::from(
                    args.get(i + 1).context("--input needs a path")?,
                ));
                i += 2;
            }
            "--out" => {
                out = PathBuf::from(args.get(i + 1).context("--out needs a path")?);
                i += 2;
            }
            "--width" => {
                width = args
                    .get(i + 1)
                    .context("--width needs a number")?
                    .parse()
                    .context("--width must be a number")?;
                i += 2;
            }
            "--check" => {
                check = true;
                i += 1;
            }
            other => bail!("unknown arg: {other}"),
        }
    }

    if check {
        return run_check(&out);
    }
    let input = input.context("--input <ne_50m_admin_0_countries.geojson> is required")?;
    generate(&input, &out, Res::new(width)?)
}

fn generate(input: &Path, out: &Path, res: Res) -> Result<()> {
    let raw = fs::read_to_string(input).with_context(|| format!("read {input:?}"))?;
    let doc: Value = serde_json::from_str(&raw)?;
    let features = doc["features"].as_array().context("no features")?;

    let mut countries: Vec<Country> = Vec::new();
    for f in features {
        let props = &f["properties"];
        let name = props["NAME_EN"]
            .as_str()
            .or(props["NAME"].as_str())
            .context("country without a name")?
            .to_string();
        let iso = props["ISO_A2_EH"]
            .as_str()
            .filter(|s| *s != "-99")
            .or(props["ISO_A2"].as_str().filter(|s| *s != "-99"))
            .unwrap_or("??")
            .to_string();
        let population = props["POP_EST"].as_f64().unwrap_or(0.0).max(0.0) as u64;
        let geom = &f["geometry"];
        let mut polygons = Vec::new();
        match geom["type"].as_str() {
            Some("Polygon") => polygons.push(parse_polygon(&geom["coordinates"])?),
            Some("MultiPolygon") => {
                for poly in geom["coordinates"].as_array().context("bad multipolygon")? {
                    polygons.push(parse_polygon(poly)?);
                }
            }
            other => bail!("unsupported geometry {other:?} for {name}"),
        }
        countries.push(Country {
            iso,
            name,
            population,
            polygons,
        });
    }
    // Stable ids: alphabetical by name.
    countries.sort_by(|a, b| a.name.cmp(&b.name));
    if countries.len() >= WATER as usize {
        bail!("too many countries for u16 grid");
    }
    eprintln!("{} countries", countries.len());

    // Rasterize. First country to paint a cell keeps it (paint order = id
    // order, deterministic).
    let mut grid = vec![WATER; res.w * res.h];
    for (id, country) in countries.iter().enumerate() {
        rasterize(&mut grid, country, id as u16, res);
    }

    // Zero-cell countries get one force-painted cell at their first polygon's
    // first vertex (they'd otherwise be unplayable).
    let mut counts = vec![0u32; countries.len()];
    for cell in &grid {
        if *cell != WATER {
            counts[*cell as usize] += 1;
        }
    }
    let mut forced: Vec<&str> = Vec::new();
    for (id, country) in countries.iter().enumerate() {
        if counts[id] > 0 {
            continue;
        }
        let (lon, lat) = country.polygons[0].0[0];
        let x = lon_to_x(lon, res).min(res.w - 1);
        let y = lat_to_y(lat, res).min(res.h - 1);
        grid[y * res.w + x] = id as u16;
        counts[id] = 1;
        forced.push(&country.name);
    }
    if !forced.is_empty() {
        eprintln!(
            "force-painted 1 cell for {} microstates: {}",
            forced.len(),
            forced.join(", ")
        );
    }

    // Areas from painted cells (cell height is constant, width shrinks with
    // cos(latitude)).
    let cell_h_km = EARTH_RADIUS_KM * std::f64::consts::PI / res.h as f64;
    let cell_w_eq_km = 2.0 * EARTH_RADIUS_KM * std::f64::consts::PI / res.w as f64;
    let mut areas = vec![0f64; countries.len()];
    let mut sums = vec![(0f64, 0f64); countries.len()];
    for y in 0..res.h {
        let lat = 90.0 - (y as f64 + 0.5) * 180.0 / res.h as f64;
        let cell_area = cell_h_km * cell_w_eq_km * lat.to_radians().cos().max(0.0);
        for x in 0..res.w {
            let cell = grid[y * res.w + x];
            if cell != WATER {
                areas[cell as usize] += cell_area;
                let s = &mut sums[cell as usize];
                s.0 += x as f64;
                s.1 += y as f64;
            }
        }
    }

    // Centers: the country's own cell nearest its cell centroid.
    let mut centers = vec![(0u16, 0u16); countries.len()];
    let mut best = vec![f64::MAX; countries.len()];
    for y in 0..res.h {
        for x in 0..res.w {
            let cell = grid[y * res.w + x];
            if cell == WATER {
                continue;
            }
            let id = cell as usize;
            let cx = sums[id].0 / counts[id] as f64;
            let cy = sums[id].1 / counts[id] as f64;
            let d = (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2);
            if d < best[id] {
                best[id] = d;
                centers[id] = (x as u16, y as u16);
            }
        }
    }

    // Adjacency from the raster: horizontally (with wraparound at the
    // antimeridian) and vertically touching cells of different countries.
    let mut pair_counts: BTreeMap<(u16, u16), u32> = BTreeMap::new();
    let mut touch = |a: u16, b: u16| {
        if a != WATER && b != WATER && a != b {
            *pair_counts.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    };
    for y in 0..res.h {
        for x in 0..res.w {
            let cell = grid[y * res.w + x];
            let right = grid[y * res.w + (x + 1) % res.w];
            touch(cell, right);
            if y + 1 < res.h {
                touch(cell, grid[(y + 1) * res.w + x]);
            }
        }
    }
    let mut neighbors: Vec<Vec<u16>> = vec![Vec::new(); countries.len()];
    for (a, b) in pair_counts.keys() {
        neighbors[*a as usize].push(*b);
        neighbors[*b as usize].push(*a);
    }
    let islands = neighbors.iter().filter(|n| n.is_empty()).count();
    eprintln!(
        "{} adjacency pairs, {} island countries (jump-only)",
        pair_counts.len(),
        islands
    );

    // Write territories JSON.
    fs::create_dir_all(out)?;
    let territories: Vec<Value> = countries
        .iter()
        .enumerate()
        .map(|(id, c)| {
            serde_json::json!({
                "id": id,
                "iso": c.iso,
                "name": c.name,
                "population": c.population,
                "area_km2": areas[id].round() as u64,
                "neighbors": neighbors[id],
                "center": [centers[id].0, centers[id].1],
            })
        })
        .collect();
    let json_path = out.join("earth_territories.json");
    fs::write(&json_path, serde_json::to_string(&territories)?)?;
    eprintln!(
        "wrote {json_path:?} ({} bytes)",
        fs::metadata(&json_path)?.len()
    );

    // Write RLE grid.
    let mut bin: Vec<u8> = Vec::new();
    bin.extend_from_slice(MAGIC);
    bin.extend_from_slice(&(res.w as u16).to_le_bytes());
    bin.extend_from_slice(&(res.h as u16).to_le_bytes());
    bin.push(0); // flags: no terrain plane yet
    bin.extend_from_slice(&(countries.len() as u16).to_le_bytes());
    let mut i = 0usize;
    while i < grid.len() {
        let cell = grid[i];
        let mut run = 1usize;
        while i + run < grid.len() && grid[i + run] == cell && run < u16::MAX as usize {
            run += 1;
        }
        bin.extend_from_slice(&(run as u16).to_le_bytes());
        bin.extend_from_slice(&cell.to_le_bytes());
        i += run;
    }
    let bin_path = out.join("earth_grid.bin");
    fs::write(&bin_path, &bin)?;
    eprintln!("wrote {bin_path:?} ({} bytes)", bin.len());
    Ok(())
}

fn parse_polygon(coords: &Value) -> Result<Polygon> {
    let rings = coords.as_array().context("bad polygon")?;
    let mut parsed: Vec<Vec<(f64, f64)>> = Vec::new();
    for ring in rings {
        let pts = ring
            .as_array()
            .context("bad ring")?
            .iter()
            .map(|p| {
                let p = p.as_array().context("bad point")?;
                Ok((
                    p[0].as_f64().context("bad lon")?,
                    p[1].as_f64().context("bad lat")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        parsed.push(pts);
    }
    let outer = parsed.first().cloned().context("empty polygon")?;
    Ok((outer, parsed.into_iter().skip(1).collect()))
}

fn lon_to_x(lon: f64, res: Res) -> usize {
    (((lon + 180.0) / 360.0 * res.w as f64) as isize).clamp(0, res.w as isize - 1) as usize
}

fn lat_to_y(lat: f64, res: Res) -> usize {
    (((90.0 - lat) / 180.0 * res.h as f64) as isize).clamp(0, res.h as isize - 1) as usize
}

/// Even-odd scanline fill of every ring (outer + holes together): a cell is
/// inside when its center crosses an odd number of ring edges.
fn rasterize(grid: &mut [u16], country: &Country, id: u16, res: Res) {
    for (outer, holes) in &country.polygons {
        let rings: Vec<&Vec<(f64, f64)>> = std::iter::once(outer).chain(holes.iter()).collect();
        let (mut min_lat, mut max_lat) = (f64::MAX, f64::MIN);
        for ring in &rings {
            for (_, lat) in ring.iter() {
                min_lat = min_lat.min(*lat);
                max_lat = max_lat.max(*lat);
            }
        }
        let y0 = lat_to_y(max_lat, res);
        let y1 = lat_to_y(min_lat, res);
        for y in y0..=y1 {
            let lat = 90.0 - (y as f64 + 0.5) * 180.0 / res.h as f64;
            let mut crossings: Vec<f64> = Vec::new();
            for ring in &rings {
                for w in 0..ring.len() {
                    let (x1, y1p) = ring[w];
                    let (x2, y2p) = ring[(w + 1) % ring.len()];
                    if (y1p <= lat && y2p > lat) || (y2p <= lat && y1p > lat) {
                        let t = (lat - y1p) / (y2p - y1p);
                        crossings.push(x1 + t * (x2 - x1));
                    }
                }
            }
            crossings.sort_by(|a, b| a.total_cmp(b));
            for span in crossings.chunks(2) {
                if span.len() < 2 {
                    continue;
                }
                let xa = lon_to_x(span[0].min(span[1]), res);
                let xb = lon_to_x(span[0].max(span[1]), res);
                for x in xa..=xb {
                    // Cell-center check keeps thin slivers honest.
                    let lon = (x as f64 + 0.5) / res.w as f64 * 360.0 - 180.0;
                    if lon >= span[0] && lon <= span[1] {
                        let cell = &mut grid[y * res.w + x];
                        if *cell == WATER {
                            *cell = id;
                        }
                    }
                }
            }
        }
    }
}

fn run_check(out: &Path) -> Result<()> {
    let json_path = out.join("earth_territories.json");
    let bin_path = out.join("earth_grid.bin");
    let territories: Vec<Value> = serde_json::from_str(&fs::read_to_string(&json_path)?)?;
    let bin = fs::read(&bin_path)?;

    anyhow::ensure!(&bin[0..4] == MAGIC, "bad magic");
    let width = u16::from_le_bytes([bin[4], bin[5]]) as usize;
    let height = u16::from_le_bytes([bin[6], bin[7]]) as usize;
    let territory_count = u16::from_le_bytes([bin[9], bin[10]]) as usize;
    anyhow::ensure!(
        territory_count == territories.len(),
        "territory count mismatch: header {territory_count} vs json {}",
        territories.len()
    );

    // Decode RLE fully; total must equal width*height, every id in range.
    let mut cells = 0usize;
    let mut seen = vec![false; territory_count];
    let mut i = 11usize;
    while i + 3 < bin.len() {
        let run = u16::from_le_bytes([bin[i], bin[i + 1]]) as usize;
        let cell = u16::from_le_bytes([bin[i + 2], bin[i + 3]]);
        anyhow::ensure!(run > 0, "zero-length run");
        if cell != WATER {
            anyhow::ensure!((cell as usize) < territory_count, "cell id out of range");
            seen[cell as usize] = true;
        }
        cells += run;
        i += 4;
    }
    anyhow::ensure!(i == bin.len(), "trailing bytes");
    anyhow::ensure!(
        cells == width * height,
        "cell total {cells} != {}",
        width * height
    );
    anyhow::ensure!(seen.iter().all(|s| *s), "some territory paints zero cells");

    // Adjacency symmetric, self-free, in-range; centers in range.
    for t in &territories {
        let id = t["id"].as_u64().unwrap() as usize;
        for n in t["neighbors"].as_array().unwrap() {
            let n = n.as_u64().unwrap() as usize;
            anyhow::ensure!(n < territory_count && n != id, "bad neighbor");
            let back = territories[n]["neighbors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x.as_u64().unwrap() as usize == id);
            anyhow::ensure!(back, "asymmetric adjacency {id} -> {n}");
        }
        let c = t["center"].as_array().unwrap();
        anyhow::ensure!(
            (c[0].as_u64().unwrap() as usize) < width && (c[1].as_u64().unwrap() as usize) < height,
            "center out of range"
        );
    }
    eprintln!(
        "check ok: {territory_count} territories, {width}x{height} grid, {} RLE bytes",
        bin.len()
    );
    Ok(())
}
