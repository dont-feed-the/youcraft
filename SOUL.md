# YouCraft soul

This file holds the mission of YouCraft and the rules its design follows. It
changes more slowly than the code; technical detail belongs in the code and
its file headers.

## Mission

YouCraft is an open-source video site written from scratch in Rust. It
behaves like YouTube, with the same pages, layout, gestures, shortcuts and
addresses, on computers and on phones, in dozens of languages, right-to-left
ones included. Its name, logo, colour and code are its own. It runs
youcraft.tube from one server at home. Invited creators publish, everyone
watches, and anyone signed in with Google likes, subscribes, comments and
keeps playlists. Codecs written in Rust, FilmCraft's, turn every upload into
renditions up to 1080p, and the original is kept. Everything a person can do
on the site, a program can do through the same commands. The recommendation
algorithm is a pure function behind a published contract, so that anyone may
one day bring their own.

- YouTube's behaviour, our own work.
- Rust from the bitstream to the pixel.
- One site for every screen and every language.
- Every action is a command.
- Recommendation is a replaceable pure function.

## Design rules

1. **Observe, never copy.** Behaviour and look come from watching YouTube;
   code, logo, icons, artwork, fonts and text are ours or openly licensed.
   Nothing may suggest that YouCraft is YouTube or belongs to Google.
2. **Rust only.** Server, pages, player, recommendation and media pipeline
   are Rust, and the browser runs our code as WebAssembly. No FFmpeg and no
   hand-written JavaScript. A C library enters only where no mature Rust one
   exists and it never reads an uploaded file, as SQLite.
3. **Uploads are hostile.** Media is decoded in a separate process with
   limits; a broken file fails its own job and nothing else.
4. **Every action is a command.** One registry of typed commands serves the
   pages, the HTTP API, the `youcraft` command line and AI agents over MCP.
   Authorization is checked in the command, never in the page.
5. **Recommendation is a pure function.** It sees only what its versioned
   contract gives it, does no I/O and builds for WebAssembly. The default
   algorithm is one implementation of that contract, with no privileged path.
6. **Exact where exactness gives truth.** Media time is an integer count of
   ticks of a track's timescale, never floating-point seconds. All renditions
   share their keyframe times, so quality switches without a seam.
7. **Fast is part of correct.** Pages are rendered on the server and
   hydrated, images come at the size they are shown, and the player starts
   low and climbs.
8. **Personal data stays home.** E-mail addresses, histories and IP
   addresses never leave the server or reach a log, and no third-party
   script, font or tracker runs in our pages.
9. **Choose the simplest sufficient design.** One binary, one database file,
   one data directory; a layer is added only when it creates an independent
   contract.
10. **Name things by what they do,** never by a current caller or an
    implementation accident.
11. **Keep each file bounded:** one mission, a header of at most 20 lines,
    split before 1,000 lines.
