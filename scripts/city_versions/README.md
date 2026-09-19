# City map versions

Snapshots of `scripts/gen_city_map.py` taken before each rework of the
street, so any earlier version can be put back in one step:

    cp scripts/city_versions/<snapshot>.py scripts/gen_city_map.py
    python3 scripts/gen_city_map.py --write
    cargo run -p late-ssh

- `v2_tiles_gen_city_map.py`: the first tile street (2026-09-19), 156
  columns, one leg with a dogleg, before it grew long. Its `--style drawn`
  is v1, the front-on plaza (2026-09-18).

The live script is the version after the last snapshot.
