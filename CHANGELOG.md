# Changelog

All notable user-visible changes to YouCraft are documented here.

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
