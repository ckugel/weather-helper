# Weather Helper

A small Rust CLI that scans a directory of Markdown notes, extracts travel metadata from YAML frontmatter, fetches daily high/low temperatures from the Open‑Meteo APIs, and writes an idempotent “Weather Forecast” section back into each note.

- Zero configuration, no API keys
- Forecast when your travel dates are near; historical proxy otherwise

## Why

I am organizing my Obisidian notebook to help me with planning my abroad trip. As apart of that I wanted the ability to run a script to generate the weather for the days I am going to be gone and at the corresponding location from data I already had recorded.

## Build and run

```bash
cargo build --release
./target/release/weather-helper <markdown-notes-root>
# If omitted, <root> defaults to the current directory (.)
```

During execution, the tool prints a line per updated note and exits with code 1 if any note failed to update; otherwise exits 0.

## Note frontmatter schema

Each Markdown file must begin with a YAML frontmatter block containing at least:

```yaml
---
city: Rome           # City name (geocoded; Italy only for now)
arrival: 2025-08-20  # YYYY-MM-DD
departure: 2025-08-25# YYYY-MM-DD
---
```

## Inserted/updated section

The tool maintains a section like this, replacing it on subsequent runs:

```markdown
## Weather Forecast
<!-- WEATHER:BEGIN -->
**Forecast 2025-08-20 → 2025-08-25**
**Range**: 35°C / 20°C

_6 days • High range 30° → 35° • Low range 18° → 20°_

| Date | High (°C) | Low (°C) |
|---|---:|---:|
| 2025-08-20 | 33 | 19 |
| 2025-08-21 | 35 | 20 |
<!-- WEATHER:END -->
```

- If a `## Weather Forecast` heading exists without a block, it is inserted.
- If no heading exists, the block is appended to the end of the file.

## How it decides forecast vs. history

- If your travel window intersects the next ~16 days from “today”, the app fetches a true forecast from the Open‑Meteo Forecast API and clamps the window to the forecast horizon.
- Otherwise, it fetches a historical proxy from the ERA5 archive for the same calendar span in the previous year. This gives a rough seasonal sense when forecasts are unavailable.

## Behavior and assumptions

- Country filter: geocoding is limited to Italy (country=IT).
- Timezone: uses the timezone returned by the geocoding API; falls back to Europe/Rome.
- Units: Celsius (°C) for daily maxima/minima.
- Idempotency: the block is delimited by `<!-- WEATHER:BEGIN -->` and `<!-- WEATHER:END -->` and safely replaced on subsequent runs.

## CLI examples

```bash
# Scan current directory
weather-helper

# Scan a notes folder
weather-helper ~/notes/travel
```

## Development

- Run
  - `cargo run -- <root>`
- Test
  - `cargo test`
- Lint
  - `cargo clippy -- -D warnings`
- Format
  - `cargo fmt --all`
- Docs (includes crate-level docs in src/main.rs)
  - `cargo doc --no-deps --open`

## Architecture overview

- Walk filesystem (walkdir) to find `*.md` files.
- Parse YAML frontmatter (serde_yaml + regex) into `NoteMeta`.
- Geocode city (Open‑Meteo Geocoding API) → `(lat, lon, tz)`.
- Choose data source:
  - Forecast: `https://api.open-meteo.com/v1/forecast`
  - Archive: `https://archive-api.open-meteo.com/v1/era5`
- Convert arrays to `DayTemp`, compute summary text, and render Markdown table.
- Upsert the block with `upsert_weather_block` to keep edits stable.

Key functions (src/lib.rs):
- `extract_meta` — read and validate YAML frontmatter
- `process_note` — orchestrate geocoding, fetch, summarize, and file update
- `geocode` — Open‑Meteo geocoding (Italy only)
- `fetch_daily` / `fetch_archive` — pull forecast / ERA5 data
- `summarize` — compute range and display strings
- `render_table` — produce Markdown table
- `upsert_weather_block` — idempotent block insert/replace

## Limitations

- Geocoding is currently restricted to Italy. Expanding to global is straightforward by dropping the `country=IT` filter.
- only 3 retries and no backoff is implemented; transient network failures will cause a note to be skipped with an error.

## Acknowledgements

- Weather data by [Open‑Meteo](https://open-meteo.com/) (no API key required).
