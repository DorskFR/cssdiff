# Development

```sh
cargo test                     # integration tests drive the binary on generated sites
cargo clippy --all-targets -- -D warnings
cargo build --release
```

The toolchain is pinned in `rust-toolchain.toml`.

## Layout

| Path | Content |
|---|---|
| `src/page.rs` | Loads a page into Blitz, resolves it at a viewport, walks elements and pseudo-elements |
| `src/net.rs` | Serves `file://` URLs from the site directory, strips `--base` |
| `src/property.rs` | Compared properties: every physical longhand, logical ones dropped |
| `src/diff.rs` | Pairs elements by structural path, compares values, memoizes per shared `ComputedValues` |
| `src/report.rs` | Groups identical changes across viewports, text and JSON output |
| `examples/dump.rs` | Dumps every computed value of a site as JSON, for engine comparisons |
| `bench/` | Validation against Chrome and timing ([benchmark.md](benchmark.md)) |

## Demo GIF

`docs/demo/` holds three fake versions of a two-page site: `main`, `refactor` (same styles, rewritten CSS and renamed classes) and `broken` (moved breakpoint, lost border color). The GIF is rendered from [demo.tape](demo.tape):

```sh
cargo build --release --target x86_64-unknown-linux-musl
docker run --rm -v "$PWD:/vhs" ghcr.io/charmbracelet/vhs docs/demo.tape
```

## Benchmark

Requires Node and a Chromium headless shell (`npx playwright install chromium-headless-shell`, or `CHROME=/path/to/chrome`).

```sh
cargo build --release
(cd bench && npm ci)
python3 bench/mutate.py SITE_DIR bench/work 24 7      # 24 seeded CSS edits
bench/run.sh SITE_DIR /base-path                      # Chrome vs cssdiff per edit
```

`run.sh` dumps Chrome's `getComputedStyle()` for the unmodified site twice to measure Chrome's own run-to-run noise, then for each edit runs both engines and writes `bench/work/results.tsv`. Engine agreement on the unmodified site:

```sh
cargo run --release --example dump -- SITE_DIR /base-path 375x812,768x1024,1440x900 > bench/work/cssdiff.json
python3 bench/agreement.py bench/work/chrome-base.json bench/work/cssdiff.json
```
