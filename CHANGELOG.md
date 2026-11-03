# Changelog

All notable user-visible changes to YouCraft are documented here.

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
