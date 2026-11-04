# Changelog

All notable user-visible changes to YouCraft are documented here.

## 0.3.1

### Fixed

- The quality a player picks follows each rendition's real bitrate: `BANDWIDTH` is the peak over
  runs of one target duration, as RFC 8216 defines it, no longer inflated by the short last segment.
- Surround sound (5.1, 7.1 and the other usual layouts) is mixed down to stereo with its centre and
  surrounds, so dialogue is no longer lost.
- A wide video at 60 frames per second (32:9, say) no longer fails processing at the file-size
  limit: each file's limit follows the bitrate its renditions may reach.
- A very wide or tall picture gets renditions no longer than 4096 pixels on a side, and a file
  that claims an absurd pixel aspect or shape is refused instead of exhausting memory.
- A video whose file holds fewer frames than it says still gets its thumbnails.

### Changed

- The studio says why a video could not be processed: too long for the site
  (`YOUCRAFT_LONGEST_VIDEO_HOURS`, 12 by default), or more than the site's limits allow, besides
  unreadable, damaged or a failure of the site.
- The same upload makes the same renditions on any machine, and they are deblocked across more of
  each picture.

## 0.3.0

### Changed

- `video.delete` answers `{}`, like every other delete, instead of `null`.
- Each command says whether it changes the site (`writes` in `site.commands` and `youcraft
  commands`); tools that only read carry `readOnlyHint` for agents over MCP.
- One account keeps at most 3 uploads open at once, together no larger than the largest file
  (`YOUCRAFT_OPEN_UPLOADS`, `YOUCRAFT_OPEN_UPLOAD_BYTES`). An upload not finished 24 hours after
  it started lapses (`YOUCRAFT_UPLOAD_HOURS`): its pieces are refused, and its video and the bytes
  it received are deleted, which frees the room it claimed.
- A piece of an upload that stalls for a minute, or arrives slower than 16 KiB/s, is dropped with
  408, and the upload page sends it again. At most 8 pieces arrive at once
  (`YOUCRAFT_UPLOAD_STREAMS`).
- A visitor's address is the one the proxy wrote last in `X-Forwarded-For`, never one a client
  may have written before it.

### Added

- A rate limit on changing the site: one account, or one address while signed out, makes at most
  30 changes in a row, then 20 a minute (`YOUCRAFT_WRITES_BURST`, `YOUCRAFT_WRITES_PER_MINUTE`);
  signing in counts as one. Over it, commands answer 429 `too_often` with `Retry-After`, and pages
  say to wait a moment.

### Fixed

- A piece of an upload is read only after its sender, its upload and its offset are checked, and
  goes to disk as it arrives: nobody can make the server hold 16 MiB per request any more.
- Uploads of different videos no longer wait for one another, and starting an upload no longer
  walks every file of the site: it counts what the database records.
- Finishing an upload works again after the server stopped between moving its file and
  recording it.
- Deleting a video while a piece of its upload arrives no longer leaves the piece's file behind;
  a file that cannot be removed is logged.

## 0.2.0

### Changed

- A blocked account still manages what is its own: it reads, pauses and clears its history, makes
  and revokes its personal tokens, changes its settings, takes back its ratings, likes and
  subscriptions, and deletes its own playlists, comments and videos. It still cannot act toward
  others. The commands that allow this require the new role `signed_in`.
- The roles an account holds (`viewer`, `creator`, `admin`) are their own type in commands
  (`admin.set_role`, invitations, `account.me`, `site.policy`), which can never name `anyone` or
  `signed_in`.
- A field that breaks a rule says which one, typed instead of in English prose:
  `{"error": "invalid", "field": "title", "rule": "too_long", "limit": 100}`, or `required`,
  `one_line`, `characters`, `malformed`, `not_offered`, and `schema` with the JSON decoder's
  `mismatch`. Pages say it in the reader's language.
- Commands refuse input fields they do not know instead of ignoring them, and their input schemas
  say so and give each text's `minLength` and `maxLength`.
- Text fields count characters as the server does (Unicode scalar values), so a title full of emoji
  is no longer cut at half its length.

### Fixed

- Titles, descriptions, names and comments can no longer hold characters that reorder the text
  around them (bidirectional overrides and isolates), nor video titles and token names control
  characters; video descriptions are trimmed like every other text.
- A database failure while looking up a session no longer signs the person out.

## 0.1.16

### Changed

- The embedded player renders without asking who is signed in, which only the site shows.

## 0.1.15

### Changed

- Who sees a video follows one rule everywhere: its channel and the site's administrators always,
  everyone else once it is ready and not private. A public video still uploading or processing no
  longer shows its page to others, and administrators see a private video's page as they could
  already see its media.
- The comments of blocked authors, and the replies to them, no longer count: a video's and a
  comment's counts match what they list, and come back when the block is lifted.
- Playlists, Watch later and liked videos count, and take their thumbnail from, only the videos
  their reader may see.

### Fixed

- A video's creator and the site's administrators can delete the comments of blocked authors.
- Replies open at once on videos with many comments.
- A command run from the command line, or the demo, no longer fails when the site writes to the
  database at the same moment.

## 0.1.14

### Changed

- A release bundle (`cargo xtask site --release`) is shrunk by binaryen's wasm-opt 116 exactly
  (`cargo install wasm-opt --version 0.116.1 --locked`), and any other version, or none, stops the
  build, so one commit always makes the same bundle; the bundle's directory is made afresh.
- GitHub runs the repository's hooks on every push to `main` and every pull request, on the Rust
  that `rust-version` names.

## 0.1.13

### Added

- `/embed/<id>` plays a video alone, with its title linking to its watch page, and any site may
  show it in a frame, as YouTube's embedded player.

## 0.1.12

### Added

- Links to the project's Discord and its source code on GitHub, in the guide's footer on wide
  screens and in the menu at every size.

## 0.1.11

### Added

- `site.policy` tells what the site grants every account: the least role of a signed-in one.

### Changed

- The settings page shows a role only above what every account has, and invites only to the
  roles above it: while everyone publishes, invitations make administrators. `invite.create`
  refuses a role every account has already.

## 0.1.10

### Fixed

- In the studio, a video's form stays open, with what was typed in it, while another video's
  progress refreshes the list.

## 0.1.9

### Changed

- YouCraft has a new logo: an amber rocket with a coral flame, flying forward as a play button
  points. It replaces the folded teal triangle in the masthead and the browser tab.
- The accent colour is amber: links and the sign-in button read in a deep amber on light pages and
  a bright amber on dark ones, and buttons filled with the accent carry dark text.

## 0.1.8

### Added

- `studio.video` gets one of your videos as the studio lists it, and the upload page uses it to
  follow the video until it plays, then links to it.

### Fixed

- Sign in opens Google's sign-in at once, instead of only after a reload.
- The upload and studio forms draw each field once: their boxes no longer nest out of line.
- A playlist's description box keeps the height of its rows.

## 0.1.7

### Changed

- Views, subscribers and likes are shown compact, as each language writes them: `1.2K views`,
  `1,2 k vues`, `1,2 Mio. Aufrufe`, `1.2万 回視聴`, `3 آلاف مشاهدة`. The number is cut down, never
  rounded up, so 1,999 views reads `1.9K`, and the words agree with the number shown.
- Other counts (comments, replies, a playlist's videos, views in the studio) are written in full
  with each language's grouping and digits: `1,234`, `1 234`, `12,34,567`, `۱٬۲۳۴`.

## 0.1.6

### Added

- A site may let every signed-in account publish without an invitation
  (`YOUCRAFT_SIGNED_IN_ROLE=creator`), and bound what all its videos together fill
  (`YOUCRAFT_STORAGE_BYTES`): an upload is refused when the room left is smaller than its file,
  and claims that room from its start for a day.

## 0.1.5

### Added

- Agents run the site's commands as tools over the Model Context Protocol at `/mcp`, signed in
  with a personal token, each seeing the tools its owner's role allows.
- `youcraft call <command> [<json>] [--site <url>]` runs a command of a site from afar with the
  token in `YOUCRAFT_TOKEN`; `youcraft commands` lists the commands with the role each requires.

### Changed

- The binary's log goes to standard error, so that what the command line prints is only its
  answer.

## 0.1.4

### Added

- Likes and dislikes on the watch page, Subscribe under every video and on channel pages, Share
  (the link goes to the clipboard) and Save, which puts a video in Watch later or a playlist, or
  in a new playlist made on the spot.
- Comments under every video, the most liked or the newest first, with replies one level deep,
  likes, editing and deleting one's own; creators delete any comment on their videos and can keep
  an account from commenting on their channel, administrators delete any comment.
- Channel pages at `/@handle`, with Home, Videos and Playlists tabs.
- Search at `/results`: channels, then videos, in any script, ignoring case and diacritics; on
  phones the search button turns the masthead into the search box.
- The subscriptions feed, the watch history (remove one video, clear it, pause it), the playlists
  page, Watch later, Liked videos, and playlists with their own page, where their owner edits,
  deletes and reorders them.
- The guide lists the channels the person subscribes to.
- Views count once a day per viewer, from when the video starts playing; the home page and the
  videos beside the one being watched follow what the viewer watched, liked and subscribed to.
- Commands `video.rate`, `video.view`, `feed.subscriptions`, `channel.get`, `channel.videos`,
  `channel.playlists`, `channel.subscribe`, `channel.subscriptions`, `channel.block`,
  `comment.list`, `comment.replies`, `comment.create`, `comment.edit`, `comment.delete`,
  `comment.like`, `history.list`, `history.remove`, `history.clear`, `history.pause`,
  `playlist.create`, `playlist.update`, `playlist.delete`, `playlist.add`, `playlist.remove`,
  `playlist.move`, `playlist.list`, `playlist.get`, `search.videos`.

## 0.1.3

### Added

- The player: YouCraft's own controls over the video, laid out as on youtube.com and
  m.youtube.com, streaming the renditions through Media Source Extensions (Managed Media Source
  on iPhone, the browser's own HLS where neither exists).
- Auto quality starts with the lowest rendition and climbs as the measured bandwidth, the buffer
  and the player's size allow; the settings menu fixes a quality or the playback speed.
- The seek bar shows what is buffered and, under the pointer, a frame from the storyboard with
  its time.
- Keyboard shortcuts as on YouTube: `k` or space, `j` and `l`, the arrows, `0`–`9`, Home and End,
  `<` and `>`, `,` and `.`, `m`, `f` and `t`; the volume is remembered.
- Theatre mode spreads the video across the page, with the list of videos moving beside the
  details; full screen fills the screen with the player and its controls.
- On phones a tap shows or hides the controls and a double tap on either side jumps ten seconds.

### Changed

- The local server answers small responses at once (`TCP_NODELAY`) instead of 40 ms later.

## 0.1.2

### Added

- Uploads: a creator chooses a file on `/upload` and gives it a title, a description and a
  visibility while it is sent in resumable pieces (`PATCH` at an offset, `HEAD` to resume, as tus
  does), up to `YOUCRAFT_UPLOAD_BYTES` (16 GiB by default).
- Processing: every upload is made into H.264 renditions at 360p, 720p and 1080p (never above the
  source), with an AAC track, as CMAF files with HLS playlists addressed by byte range, keyframes
  every two seconds at the same exact ticks in every rendition; plus thumbnails at every card size
  and a storyboard for the seek bar. All through FilmCraft's codecs, in a child process
  (`youcraft transcode`) limited in memory, CPU time and file size, one job at a time, resumed
  after a restart.
- The studio, `/studio`: the channel's videos in every state, with upload and processing progress,
  why processing failed, and a form to change a video's title, description, visibility and
  comments, or to delete it.
- Media is served only to those who may watch the video: a private video's to its channel.
- `youcraft demo` generates its videos in code (a moving pattern that spells its frame numbers,
  and a tone) and makes them playable as any upload.
- Commands: `upload.create`, `upload.status`, `upload.finish`, `video.update`, `video.delete`,
  `studio.videos`.

## 0.1.1

### Added

- Sign-in with Google: an account and its channel on the first sign-in, a session in a cookie for
  180 days, and signing out from the account menu.
- The account menu, as YouTube's: the channel, the studio for creators, signing out, appearance
  (device, dark or light) and language, kept in the account, or in the browser when signed out.
- The settings page, `/account`: the account, appearance, language, personal tokens for programs,
  and the invitations waiting, for administrators.
- Invitations by e-mail address, which make creators and administrators when the person signs in
  with Google; administrators change roles and block accounts.
- The operator's command line: `youcraft admin`, `invite`, `block`, `unblock` and `run`.
- Commands `account.me`, `account.settings`, `account.signout`, `token.create`, `token.list`,
  `token.revoke`, `invite.create`, `invite.list`, `invite.revoke`, `admin.set_role`, `admin.block`.

### Changed

- Commands posted by a browser count its session only from the site's own pages, and only as JSON.

## 0.1.0

### Added

- The home page: a grid of videos, ordered by the default recommendation algorithm, with the
  guide down the side on wide screens and along the bottom on phones.
- The watch page: the video, its title, channel, counts and description, and what to watch next.
- The interface in English and 41 more languages, chosen from the browser, right to left for
  Arabic, Hebrew and Persian; light and dark themes from the device.
- Commands over HTTP at `POST /api/v1/<id>`: `site.commands`, `feed.home`, `video.get`,
  `video.next`.
- `youcraft serve`, under systemd socket activation or on a TCP address, and `youcraft demo`.
