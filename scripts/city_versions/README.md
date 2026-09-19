# City map versions

Snapshots of `scripts/gen_city_map.py` taken before each rework of the
street, so any earlier version can be put back in one step:

    cp scripts/city_versions/<snapshot>.py scripts/gen_city_map.py
    python3 scripts/gen_city_map.py --write
    cargo run -p late-ssh

- `v2_tiles_gen_city_map.py`: the first tile street (2026-09-19), 156
  columns, one leg with a dogleg, before it grew long. Its `--style drawn`
  is v1, the front-on plaza (2026-09-18).

- `v3_long_street_gen_city_map.py`: the long tile street (2026-09-19),
  440 columns, three legs, canal, back lane, walkers, running. The one
  you said you loved. Taken before the lighting pass; it still carries
  the drawn register as `--style drawn`, and the renderer of that time
  is in git history beside it (the lit renderer needs `LIGHTS`, which
  this script does not emit).

- `v4_lit_gen_city_map.py`: the lit street (2026-09-19): emits `LIGHTS`,
  the drawn register gone. Taken before the ledge, the billboards, the
  traffic and the brighter night. Renders with the lit renderer from git
  history (no `BILLBOARDS`, no `Ledge` landmark yet).

The live script is the version after the last snapshot.
