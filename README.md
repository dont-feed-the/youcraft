# YouCraft

An open-source video site written from scratch in Rust, with YouTube's layout and behaviour under
its own name, logo and colour. It runs [youcraft.tube](https://youcraft.tube).

[SOUL.md](SOUL.md) holds its mission and design rules, and [AGENTS.md](AGENTS.md) the rules for
everyone who changes the code, people and agents alike.

## Run it locally

```sh
rustup target add wasm32-unknown-unknown
cargo run -p youcraft-server -- demo   # example channels, and videos made in code, in data/
cargo xtask serve                      # http://127.0.0.1:3100
```

`YOUCRAFT_ADDRESS`, `YOUCRAFT_DATA` and `YOUCRAFT_SITE` change where it listens, keeps its data and
finds the browser bundle; `YOUCRAFT_ORIGIN` is the address people reach it at
(`http://localhost:3100` by default). Every command of the site answers at `POST /api/v1/<id>`;
`site.commands` lists them all with their schemas. Programs sign in with a personal token, made on
the settings page and sent as `Authorization: Bearer <token>`.

## Sign-in and the first administrator

People sign in with Google. Give the site its OAuth client ID in `YOUCRAFT_GOOGLE_CLIENT_ID`, and in
`YOUCRAFT_GOOGLE_KEYS` a file of Google's signing keys (`https://www.googleapis.com/oauth2/v3/certs`,
refreshed by a timer, since the site itself never reaches the network); register
`<origin>/signin/google` as a redirect URI. Then, on the server:

```sh
youcraft admin you@example.com           # an administrator, from their first sign-in
youcraft invite friend@example.com       # a creator, who may publish
youcraft block @handle                   # or unblock
youcraft run <command id> '<json>'       # any command, as the operator
```

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
