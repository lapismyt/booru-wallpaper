# booru-wallpaper

A CLI tool that fetches random wallpapers from booru imageboards and sets them as your desktop wallpaper.

Supported imageboards:

- `safebooru`
- `gelbooru`
- `rule34`
- `danbooru`

Supported wallpaper backends:

- `wallpaper` - default backend; on Wayland it usually goes through `swaybg`
- `awww` - Wayland backend

## Features

- tag and blacklist-tag based search
- `rating` filtering
- minimum score filtering
- post sorting
- periodic wallpaper rotation
- batch candidate selection before retries
- retries for network errors and unsuitable candidates
- TOML config support
- CLI overrides for config values
- dry-run mode
- animated content handling for both `awww` and `wallpaper`
- softer wallpaper-shape filtering via minimum dimensions and aspect ratio range

## Requirements

- Rust and Cargo to build the project
- internet access
- one wallpaper backend:
  - `wallpaper` for regular wallpaper setting, requires `feh` on `i3` and `swaybg`, not supports animated wallpaper.
  - `awww` for for AWWW on Wayland, supports animated wallpaper.
- `ffmpeg` and `ffprobe` for handling `mp4`/`webm` posts

`ffmpeg` is used as follows:

- for `awww`, video is converted into GIF
- for `wallpaper`, a static PNG frame is extracted from video

## Installation

Build from source:

```bash
cargo build --release
```

The binary will be available at:

```bash
target/release/booru-wallpaper
```

Run locally during development:

```bash
cargo run -- --help
```

## Quick Start

Set a random safe wallpaper once:

```bash
cargo run -- -t "wallpaper landscape"
```

Use `awww`:

```bash
cargo run -- -t "animated wallpaper" --wallpaper-setter awww
```

Retry up to 5 times after the first failure:

```bash
cargo run -- -t "wallpaper" --max-retries 5
```

Fetch a batch of 100 posts per attempt and wait 3 seconds between retries:

```bash
cargo run -- -t "wallpaper" --batch-size 100 --retry-interval-seconds 3
```

Relax wallpaper matching criteria:

```bash
cargo run -- -t "wallpaper" --wallpaper-min-width 1366 --wallpaper-min-height 768 --wallpaper-aspect-ratio-min 1.5 --wallpaper-aspect-ratio-max 2.2
```

Only print the selected URL:

```bash
cargo run -- -t "wallpaper" --dry-run
```

Rotate wallpaper every 5 minutes:

```bash
cargo run -- -t "wallpaper" -c 300
```

## Configuration

By default, the config is read from the standard application config directory:

- Linux: the path from `directories::ProjectDirs`, usually `~/.config/booru-wallpaper/config.toml`
- Windows: a path under `AppData`

If the default config file does not exist, the application creates a template automatically.

You can pass a custom config path:

```bash
booru-wallpaper /path/to/config.toml
```

You can disable config loading and use CLI arguments only:

```bash
booru-wallpaper none -t "wallpaper"
```

Example `config.toml`:

```toml
tags = ["wallpaper"]
# blacklist_tags = []
# min_score = 0
# cycle_interval_seconds = 300
# imageboard = "safebooru"  # danbooru, safebooru, gelbooru, rule34
# rating = "safe"  # safe, questionable, explicit
# sort_by = "random"  # random, id, score, rating, user, height, width, source, updated
# user_id = ""  # required for gelbooru and rule34
# api_key = ""  # required for gelbooru and rule34
# max_retries = 3
# retry_interval_seconds = 2
# batch_size = 100
# disable_resolution_filter = false
# wallpaper_setter = "wallpaper"  # wallpaper, awww
# wallpaper_min_width = 1600
# wallpaper_min_height = 900
# wallpaper_aspect_ratio_min = 1.6
# wallpaper_aspect_ratio_max = 2.1
# animated_max_duration_seconds = 12
# animated_fps = 10
# animated_width = 1280
```

## Wallpaper Criteria

If `disable_resolution_filter = false`, the application uses two filtering stages:

- for `gelbooru`, `rule34`, and `safebooru`, it adds `width:>=...` and `height:>=...` to the booru query
- when the API response includes post dimensions, candidates are filtered by post metadata before download
- if dimensions are not available in metadata, the downloaded file is validated locally as a fallback

Default criteria:

- `wallpaper_min_width = 1600`
- `wallpaper_min_height = 900`
- `wallpaper_aspect_ratio_min = 1.6`
- `wallpaper_aspect_ratio_max = 2.1`

This is softer than the old exact `1920x1080` requirement and fits real-world wallpaper formats better.

If `--disable-resolution-filter` is enabled, both booru-side filtering and local dimension checks are disabled.

## Retries

`max_retries` defines how many retries happen after the first failed attempt.
`retry_interval_seconds` defines the delay between retry attempts.

Before each retry, the application fetches a batch of posts and checks candidates from that batch one by one until it finds a suitable wallpaper or exhausts the batch.

`batch_size` defines how many posts are fetched per attempt.

A retry is triggered if any of the following fails:

- imageboard request
- every candidate in the fetched batch is unsuitable or fails during processing
- animated-content preparation
- wallpaper setting

So:

- `max_retries = 0` means one attempt with no retries
- `max_retries = 3` means up to 4 total attempts
- `batch_size = 100` means each attempt checks up to 100 fetched posts before retrying

## Animated Content

Animated content is no longer excluded automatically just because of the selected backend.

Backend behavior:

- `awww`
  - regular images are passed through directly
  - `video/mp4` and `video/webm` are converted to GIF with `ffmpeg`
- `wallpaper`
  - regular images are passed through directly
  - `video/mp4` and `video/webm` are converted into a static PNG frame with `ffmpeg`

Animated preparation parameters:

- `animated_max_duration_seconds`
- `animated_fps`
- `animated_width`

These parameters control video preparation before wallpaper application. For `awww`, they affect the generated GIF. For `wallpaper`, `animated_width` currently affects the extracted frame width.

## CLI

```text
Usage: booru-wallpaper [OPTIONS] [CONFIG]

Arguments:
  [CONFIG]  Path to the base config file. Can be disabled with "none" to use only CLI args. By default, uses ~/.config on UNIX and AppData on Windows [default: default]

Options:
  -i, --imageboard <IMAGEBOARD>
          Imageboard to use. "safebooru" by default [possible values: danbooru, gelbooru, rule34, safebooru]
  -m, --min-score <MIN_SCORE>
          Minimum score filter
  -t, --tags <TAGS>
          Tags to search for
  -B, --blacklist-tags <BLACKLIST_TAGS>
          Ignore images with these tags
  -r, --rating <RATING>
          Safety rating [possible values: safe, questionable, explicit]
  -c, --cycle-interval-seconds <CYCLE_INTERVAL_SECONDS>
          Cycle interval in seconds. Runs once if not set
  -a, --api-key <API_KEY>
          API key for the imageboard
  -u, --user-id <USER_ID>
          User ID for the imageboard
  -s, --sort-by <SORT_BY>
          Posts sort_by option [possible values: random, id, score, rating, user, height, width, source, updated]
  -D, --disable-resolution-filter
          Disable resolution filtering tags
  -w, --wallpaper-setter <WALLPAPER_SETTER>
          Wallpaper setter backend. "wallpaper" by default [possible values: wallpaper, awww]
  -R, --max-retries <MAX_RETRIES>
          Maximum retries after the first failed attempt. 3 by default
  -I, --retry-interval-seconds <RETRY_INTERVAL_SECONDS>
          Delay between retries in seconds. 2 by default
  -b, --batch-size <BATCH_SIZE>
          Number of posts fetched per attempt before retrying. 100 by default
  -W, --wallpaper-min-width <WALLPAPER_MIN_WIDTH>
          Minimum wallpaper width. 1600 by default
  -E, --wallpaper-min-height <WALLPAPER_MIN_HEIGHT>
          Minimum wallpaper height. 900 by default
  -n, --wallpaper-aspect-ratio-min <WALLPAPER_ASPECT_RATIO_MIN>
          Minimum wallpaper aspect ratio. 1.6 by default
  -x, --wallpaper-aspect-ratio-max <WALLPAPER_ASPECT_RATIO_MAX>
          Maximum wallpaper aspect ratio. 2.1 by default
  -T, --animated-max-duration-seconds <ANIMATED_MAX_DURATION_SECONDS>
          Maximum duration in seconds used when preparing animated wallpapers. 12 by default
  -F, --animated-fps <ANIMATED_FPS>
          FPS used when preparing animated wallpapers. 10 by default
  -P, --animated-width <ANIMATED_WIDTH>
          Output width used when preparing animated wallpapers. 1280 by default
  -d, --dry-run
          Dry run - only print the image URL, don't set it on the wallpaper
  -h, --help
          Print help
  -V, --version
          Print version
```

## Notes

- `gelbooru` and `rule34` may require `user_id` and `api_key`.
- For `danbooru`, booru-side width/height tags are not added, but local post-download size validation still applies, to prevent hitting tag count limit (danbooru allows max 2 tags).
- If `ffmpeg` or `ffprobe` is missing, animated video content cannot be prepared.

## License

MIT License
