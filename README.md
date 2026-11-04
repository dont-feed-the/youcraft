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

A release bundle (`cargo xtask site --release`, as the live site is built) is shrunk by binaryen's
wasm-opt, version 116 exactly, so that one commit always makes the same bundle:
`cargo install wasm-opt --version 0.116.1 --locked`.

`YOUCRAFT_ADDRESS`, `YOUCRAFT_DATA` and `YOUCRAFT_SITE` change where it listens, keeps its data and
finds the browser bundle; `YOUCRAFT_ORIGIN` is the address people reach it at
(`http://localhost:3100` by default). Every command of the site answers at `POST /api/v1/<id>`;
`site.commands` lists them all with their schemas. Programs sign in with a personal token, made on
the settings page and sent as `Authorization: Bearer <token>`.

## Programs and agents

Make a personal token on the settings page (`/account`, under Personal tokens); it can do exactly
what its owner can. The same commands then answer three ways:

```sh
export YOUCRAFT_TOKEN=yct_…                        # without it, as someone signed out
youcraft commands                                  # every command, with the role it requires
youcraft call search.videos '{"query": "waves"}'   # at https://youcraft.tube, or --site <url>
curl -H "Authorization: Bearer $YOUCRAFT_TOKEN" -H 'Content-Type: application/json' \
     -d '{}' https://youcraft.tube/api/v1/account.me
```

Agents reach them as tools over the Model Context Protocol at `https://youcraft.tube/mcp`
(Streamable HTTP, with the token as `Authorization: Bearer`); each tool is a command, named by its
id with `-` for `.`, such as `video-get`. For Claude Code:

```sh
claude mcp add --transport http youcraft https://youcraft.tube/mcp \
  --header "Authorization: Bearer $YOUCRAFT_TOKEN"
```

## Sign-in and the first administrator

People sign in with Google. Give the site its OAuth client ID in `YOUCRAFT_GOOGLE_CLIENT_ID`, and in
`YOUCRAFT_GOOGLE_KEYS` a file of Google's signing keys (`https://www.googleapis.com/oauth2/v3/certs`,
refreshed by a timer, since the site itself never reaches the network); register
`<origin>/signin/google` as a redirect URI. Then, on the server:

```sh
youcraft admin you@example.com           # an administrator, from their first sign-in
youcraft invite friend@example.com       # a creator, who may publish
youcraft block <account>                 # or unblock; admin.accounts lists them
youcraft run <command id> '<json>'       # any command, as the operator
```

To let every account publish without an invitation, set `YOUCRAFT_SIGNED_IN_ROLE=creator`, and
bound what all videos together may fill with `YOUCRAFT_STORAGE_BYTES`; the role of an account,
when higher, and blocking still apply.

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
