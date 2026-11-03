# YouCraft

An open-source video site written from scratch in Rust, with YouTube's layout and behaviour under
its own name, logo and colour. It runs [youcraft.tube](https://youcraft.tube).

[SOUL.md](SOUL.md) holds its mission and design rules, and [AGENTS.md](AGENTS.md) the rules for
everyone who changes the code, people and agents alike.

## Run it locally

```sh
rustup target add wasm32-unknown-unknown
cargo run -p youcraft-server -- demo   # example channels and videos in data/
cargo xtask serve                      # http://127.0.0.1:3100
```

`YOUCRAFT_ADDRESS`, `YOUCRAFT_DATA` and `YOUCRAFT_SITE` change where it listens, keeps its data and
finds the browser bundle. Every command of the site answers at `POST /api/v1/<id>`;
`site.commands` lists them all with their schemas.

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
