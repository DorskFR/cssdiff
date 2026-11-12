# cssdiff

Diff the computed CSS styles of every element of two versions of a site, at every viewport, without a browser.

[![release](https://img.shields.io/github/v/release/DorskFR/cssdiff)](https://github.com/DorskFR/cssdiff/releases)
[![CI](https://github.com/DorskFR/cssdiff/actions/workflows/ci.yml/badge.svg)](https://github.com/DorskFR/cssdiff/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/DorskFR/cssdiff)](LICENSE)

![cssdiff finding no change in a CSS refactor, then a moved breakpoint and a lost border color](docs/assets/demo.gif)

Takes two builds of a static site, or two running servers, resolves the full cascade of every page with Firefox's CSS engine ([Stylo](https://github.com/servo/stylo), through [Blitz](https://github.com/DioxusLabs/blitz)) and reports every element whose computed style changed, per viewport. A CSS regression check for CI with no Chromium, no screenshots and no pixel thresholds: one static binary.

```sh
cssdiff main/dist branch/dist                 # every page, three default viewports
cssdiff -v 390x844,1280x800 a.html b.html     # two files, custom viewports
cssdiff -b /docs --json old/ new/             # site served under /docs, JSON report
cssdiff -r routes.txt http://localhost:3001 http://localhost:3002   # server-rendered site
```

## Install

```sh
curl -fsSL -o cssdiff https://github.com/DorskFR/cssdiff/releases/latest/download/cssdiff-linux-amd64
chmod +x cssdiff
```

Binaries for `linux-amd64`, `linux-arm64` and `darwin-arm64` are on the [releases page](https://github.com/DorskFR/cssdiff/releases). Container image: `ghcr.io/dorskfr/cssdiff`.

## Quick start

```console
$ git clone https://github.com/DorskFR/cssdiff && cd cssdiff
$ cssdiff -p grid-template-columns,border-top-color docs/demo/main docs/demo/broken
pricing.html
  html > body > main.content > ul.plans
    grid-template-columns: repeat(3, 1fr) → 1fr  (768x1024)
  html > body > main.content > ul.plans > li.plan.featured[2]
    border-top-color: rgb(11, 95, 255) → rgb(208, 215, 222)  (all viewports)
2 page(s) × 3 viewport(s) × 2 properties, 34 elements: 2 computed-style difference(s) in 5ms
```

Exit status: `0` identical, `1` differences, `2` error.

## Usage

### Gate a pull request

```yaml
- uses: actions/checkout@v6
- uses: actions/checkout@v6
  with: { ref: main, path: base }
- run: npm ci && npm run build && (cd base && npm ci && npm run build)
- run: cssdiff base/dist dist
```

### Compare a server-rendered site

Sites rendered per request (Next.js, Remix, Rails) have no HTML on disk. Run both production servers and pass the routes:

```sh
(cd main && next build && next start -p 3001) &
(cd branch && next build && next start -p 3002) &
cssdiff --routes /,/pricing,/blog/hello http://localhost:3001 http://localhost:3002
```

`--routes` takes `/path` values or a file of routes, one per line. An HTTP status change counts as a difference. `url()` values pointing at the server itself are compared by path, so the two ports never differ. Stub backend APIs with fixtures so both servers render the same data, fast.

### Compare layout values too

```sh
cssdiff --mode used before/ after/
```

`--mode used` returns what `getComputedStyle()` returns: `width`, `height`, margins, padding and insets as laid-out pixels.

### Check interaction states

```sh
cssdiff --hover --focus --active before/ after/
```

Each flag adds a pass with that state forced on every element at once. Changes that appear only in that pass are reported as `:hover only` (`1440x900:hover` in JSON).

## Configuration

| Option | Default | Effect |
|---|---|---|
| `-v, --viewport` | `375x812,768x1024,1440x900` | Viewports compared |
| `-b, --base` | none | URL prefix stripped from root-relative links |
| `-r, --routes` | none | Routes fetched from two `http://` servers |
| `--timeout` | `30` | Seconds allowed per HTTP request |
| `-p, --property` / `-i, --ignore` | all 249 physical longhands | Properties compared |
| `-m, --mode` | `computed` | `computed` or `used` values |
| `--hover`, `--focus`, `--active` | off | Also compare with that state forced on every element |
| `-j, --jobs` | CPU count | Pages processed in parallel |

The full list is in `cssdiff --help`.

## How it works

Each page is parsed into Blitz's headless DOM. `<link>` and `@import` load from disk. Stylo computes the cascade per viewport: specificity, `!important`, `@layer`, `@media`, `@supports`, custom properties, inheritance, `::before` and `::after`. Elements pair by structural path (`/html[1]/body[1]/main[1]`), not class names, so renamed classes with unchanged styles produce no diff.

On 24 random CSS edits to a real 28-page site, cssdiff found all 549 non-layout computed-style changes Chrome's `getComputedStyle()` found, and none it did not, in 0.42 s per comparison against 71 s per version for Chrome: [docs/benchmark.md](docs/benchmark.md).

## Limitations

- No JavaScript runs. Client-rendered pages need a pre-rendered build or a server render.
- Servers are fetched over `http://` only, without compression.
- States are forced on every element at once: `:not(:hover)` never matches, and competing state rules on nested elements can mask each other.
- `:focus-visible` and `:focus-within` never match.
- Values follow Firefox's engine; a feature Stylo lacks is invisible on both sides.
- `--mode used` lays out without system fonts: text-dependent sizes differ from a browser but stay deterministic.

## Development

```sh
git clone https://github.com/DorskFR/cssdiff && cd cssdiff
cargo test
```

## License

[MIT](LICENSE)
