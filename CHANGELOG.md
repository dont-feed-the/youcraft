# Changelog

All notable user-visible changes to YouCraft are documented here.

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
