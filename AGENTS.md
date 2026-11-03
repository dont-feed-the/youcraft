# AGENTS.md: instructions for agents

YouCraft is an open-source video site written from scratch in Rust, with YouTube's layout and
behaviour under its own name, logo and colour. It runs https://youcraft.tube from rtx-home; the
repository is https://github.com/dont-feed-the/youcraft. The product name is written **YouCraft**
in everything people read (pages, titles, docs prose); machine names stay lowercase (crates
`youcraft-*`, the binary `youcraft`, files, ids). [`SOUL.md`](SOUL.md) is the authority on product
and code design: read it first.

## 1. Workspace map

```text
crates/
  api        identifiers, records, routes and the command registry shared by browser and server
  i18n       the catalogs of locales/: lookup, plurals, numbers, durations, relative dates
  recommend  the versioned recommendation contract and the default algorithm: pure, no I/O
  player     the video player over <video> and Media Source Extensions
  ui         every page and component (Leptos, rendered on the server, hydrated in the browser)
  store      the SQLite database: schema, migrations, queries, full-text search
  transcode  uploads to renditions, thumbnails and storyboards, through FilmCraft's codecs
  server     the `youcraft` binary: pages, commands over HTTP, uploads, media, jobs, sign-in, CLI, MCP
locales/     one catalog per language, in craft-i18n's TSV format
```

Dependencies point one way. `api`, `i18n` and `recommend` depend on nothing here; `player` on
`api` and `i18n`; `ui` on those and `player`; `store` and `transcode` on `api`; `server` on all.
Only `store`, `transcode` and `server` touch files, the database, processes or the network. A crate
that runs in the browser declares `[package.metadata.youcraft] wasm = true` (with `wasm-features`
if it needs some), and `scripts/check-wasm` builds it for `wasm32-unknown-unknown`.

## 2. Golden rules

### Never crash (outranks feature work)
- Non-test code never panics: no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`, and no indexing that can fall out of bounds. The root `Cargo.toml` denies them
  in `[workspace.lints]`, every crate takes `[lints] workspace = true`, and `unsafe_code = "forbid"`.
- Errors are `Result`s of the crate's error type, propagated with `?`. A failed request answers
  with a page or status the person can act on, in their language, and the server keeps serving.
- Input is hostile: uploads, form fields, command JSON, cookies, headers (`Range` included), tokens
  and catalogs. Use `get()`, checked or saturating arithmetic on sizes and offsets, cap every
  allocation by configuration, and bound every recursion and loop.
- Media is decoded in a child process (`youcraft transcode`) with memory, CPU-time and output
  limits. A codec failure fails its job and tells the creator; the site stays untouched.
- Every crash fix lands with a regression test in the existing test file of its area, never in a
  new file named after the bug.

### Clean-room
- We study youtube.com and m.youtube.com for behaviour and look only: layout, proportions, motion,
  gestures, keyboard shortcuts, menus, the URL scheme. Never copy their code (HTML, CSS,
  JavaScript), logo, icons, illustrations, sounds, fonts or text; never put their files or our
  captures of them in the repository; never fetch YouTube's content into ours.
- YouCraft's logo, accent colour and wording are its own. Nothing may suggest that YouCraft is
  YouTube or is run by Google.
- A third-party asset (icon set, font) is openly licensed, keeps its licence file next to it, and
  gets a row in `ATTRIBUTION.md` (path, title, author, source, licence) in the same change. Demo and
  test media are public domain or generated in code; never personal footage.

### Codecs are FilmCraft's
- Decoding, scaling and encoding go through the codec crates of
  [storytold/filmcraft](https://github.com/storytold/filmcraft), pinned by an immutable `rev`. No
  FFmpeg, GStreamer or C codec in the product or its image; ffmpeg may only serve as an outside
  oracle in local checks.
- When a codec fails on a real file, fix it at the source: a pull request to FilmCraft with the
  file reduced to a test, then move the pin. Never work around a codec bug here.

### Every action is a command
- Each action (upload, publish, edit, like, subscribe, comment, playlists, history, moderation,
  invitations) is a command in `api`, with an id, typed input and output, and the role it
  requires; `server` runs it. The pages, `POST /api/v1/<id>`, the CLI and the MCP server dispatch
  the same commands, and a new action never gets a page-only path.
- Authorization happens inside the command, on the server; a page only hides what a person cannot
  do. Programs authenticate with personal tokens, which can do exactly what their owner can.

### Recommendation is a contract
- `recommend` defines, under a version, what an algorithm receives (the viewer's own history,
  likes and subscriptions; candidate videos with their public features) and what it returns (an
  ordered list of candidate ids). It does no I/O and builds for wasm32, so that a WebAssembly
  module can one day implement the same contract.
- The default algorithm implements that contract with no privileged access. Changing the contract
  means adding a version beside the old one.

### Every string is translated
- Everything people read comes from the catalogs in `locales/` (craft-i18n's TSV format); English
  is the source. No user-visible literal in code.
- A new or changed string is translated into every catalog in the same change. Plurals, numbers,
  view counts, durations and relative dates go through `i18n`, never `format!`.
- The languages include Arabic, Hebrew and Persian: CSS uses logical properties only
  (`inline-start`, never `left`), and every page is checked right-to-left.

### The interface
- One set of components lays out as youtube.com on wide screens and as m.youtube.com on phones.
  Check every page you touch at 1440×900 and 390×844, light and dark, left-to-right and
  right-to-left.
- Colours, spacing, radii and type sizes come from the design tokens (CSS custom properties for
  each theme), never a literal in a component.
- Nothing third-party loads in our pages: no script, font, image or tracker. Sign-in redirects to
  Google and back; no Google script runs here. wasm-bindgen's generated glue is the only
  JavaScript.
- Look at what you changed: render the page with Playwright's Chromium, and its WebKit for Safari
  (on rtx-home both are in `~/.cache/ms-playwright`), and open the PNGs. Compare with the same page
  of YouTube observed live; keep those captures out of the repository.

### Media time is exact
- Timestamps and durations are integers in a track's timescale, converted with exact rational
  arithmetic; never floating-point seconds.
- Every rendition of a video has its keyframes at the same times, so the player switches quality
  without a seam.
- Media tests use videos generated in code with known frames, tones and durations, and assert
  exact timestamps, keyframe alignment and a PSNR floor.

### Privacy and speed
- E-mail addresses, histories and IP addresses never appear in logs, URLs or the repository, and
  data from the live site never enters the repository or a test.
- Pages render on the server and hydrate, images come at the size they are shown, and the player
  starts with a low rendition. Measure a hot path (page weight, server time, time to first frame,
  transcode speed) before and after changing it, and put the numbers in the commit message.

## 3. Before you finish

Install the hooks once with `pre-commit install`. They run on every commit: file length and header
(`scripts/check-file-length-and-header`), `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, `cargo test` and `scripts/check-wasm`. Run `pre-commit run --all-files` before you
merge, and never skip or weaken a hook to get a commit through.

## 4. Landing a change

- Start from a clean `main`, pulled from `origin`.
- A single-file change may go straight to `main`. Anything larger goes on a branch in a git
  worktree in your scratch directory, with its own `CARGO_TARGET_DIR`: commit there with the
  hooks, merge onto `main`, then remove the worktree and the branch.
- Agents on different tasks work in parallel. Work lands in the order it finishes, the later one
  rebases, and nobody edits another agent's files in progress.
- Every push bumps the workspace version (0.x: a breaking change bumps the second number and
  resets the third, anything else bumps the third) and adds each user-visible change to
  `CHANGELOG.md`.
- Commit no design documents (plans, proposals, write-ups); the reason for a change goes in its
  commit message.
- Once a change is on `main`, push it and deploy that exact commit.

## 5. The live site

- youcraft.tube is served by rtx-home's `geiger` account, set up in `~/git/geiger-server` (read its
  `README.md`). Caddy proxies to the container `youcraft`, which has no network and mounts only its
  data, `~geiger/data/youcraft/`, and Google's public keys, `~geiger/data/google-keys/`.
- Deploy with `./deploy youcraft` in `~/git/geiger-server`, then check that
  `https://youcraft.tube/health` answers 200.
- Live data never leaves the server; back up the database before any migration runs on it.
