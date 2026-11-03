# Changelog

All notable user-visible changes to YouCraft are documented here.

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
