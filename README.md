<img src=".github/banner.png#1" alt="enstate.rs" />

> ⚠️ This branch is currently undergoing a rewrite.
> Please see the [v1](https://github.com/v3xlabs/edgeserver/tree/v1) branch for the latest stable release.

## 📌 API Specification (OpenAPI)

The API specification is available [on enstate.rs](https://enstate.rs/docs) or locally at `{your_site_here}/docs`.

## 🌐 Hosted version

For demonstration purposes (and one-off usage), a hosted instance is made available at [https://enstate.rs](https://enstate.rs) and a cloudflare worker at [https://worker.enstate.rs](https://worker.enstate.rs). This instance is provided as-is and as a gift to the community. Please do not abuse it.

### 📌 Example

> [name/luc.eth](https://worker.enstate.rs/n/luc.eth) &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; [name/rescueorg.eth](https://worker.enstate.rs/n/rescueorg.eth) &nbsp;&nbsp;&nbsp; [name/antony.sh](https://worker.enstate.rs/n/antony.sh)<br />
> [image/vitalik.eth](https://worker.enstate.rs/i/vitalik.eth)&nbsp;&nbsp;&nbsp; [name/khori.eth](https://worker.enstate.rs/n/khori.eth) &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; [name/helgesson.eth](https://worker.enstate.rs/n/helgesson.eth)<br />
> [bulk/address](https://enstate.rs/bulk/a?addresses[]=0x225f137127d9067788314bc7fcc1f36746a3c3B5&addresses[]=0xd577D1322cB22eB6EAC1a008F62b18807921EFBc&addresses[]=0x8F8f07b6D61806Ec38febd15B07528dCF2903Ae7&addresses[]=0x8e8Db5CcEF88cca9d624701Db544989C996E3216&addresses[]=0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5&addresses[]=0xF1F78f308F08fDCAC933124ee8B52A376ff542B4) &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; [sse/address](https://enstate.rs/sse/a?addresses[]=0x225f137127d9067788314bc7fcc1f36746a3c3B5&addresses[]=0xd577D1322cB22eB6EAC1a008F62b18807921EFBc&addresses[]=0x8F8f07b6D61806Ec38febd15B07528dCF2903Ae7&addresses[]=0x8e8Db5CcEF88cca9d624701Db544989C996E3216&addresses[]=0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5&addresses[]=0xF1F78f308F08fDCAC933124ee8B52A376ff542B4) &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; [header/luc.eth](https://enstate.rs/h/luc.eth)

## 🚀 Getting Started

We believe software should be simple and containerized. Enstate provides you with a lightweight docker container that you can run anywhere.

### 🐳 Docker

```sh
docker run \
  -p 3000:3000 \
  -e REDIS_URL=redis://0.0.0.0:6379 \
  -e RPC_URL=https://rpc.ankr.com/eth \
  ghcr.io/v3xlabs/enstate:1.0.5
```

### 🐳 Docker Compose

```yaml
version: "3.8"
services:
    enstate:
        image: ghcr.io/v3xlabs/enstate:1.0.5
        ports:
            - 3000:3000
        environment:
            - REDIS_URL=redis://redis:6379
            - RPC_URL=https://rpc.ankr.com/eth
        depends_on:
            - redis
    redis:
        image: redis:6.2.5-alpine
        ports:
            - 6379:6379
```

### 🦀 Cloudflare Workers

```sh
cd worker
```

#### Run the worker locally

```sh
cp .dev.vars.example .dev.vars
```

Edit your `.dev.vars` file at this time to include environment variables for `UNIVERSAL_RESOLVER`, `RPC_URL` (optional) and `OPENSEA_API_KEY` (optional).

To run the worker locally you can now run:

```
pnpm dev
```

#### Deploying to Cloudflare Workers

Create a [**KV namespace**](https://developers.cloudflare.com/kv/get-started/#3-create-a-kv-namespace) via `wrangler` or the Cloudflare dashboard.

```sh
pnpm wrangler kv:namespace create <YOUR_NAMESPACE>
```

Copy the `id` of your newly created KV namespace to your [`wrangler.toml`](./worker/wrangler.toml). The `binding` value should remain as `enstate-1` regardless of what you named yours when you created it.

Deploy the worker:

```sh
pnpm wrangler deploy
```

Upload your secrets:

```sh
echo "https://rpc.ankr.com/eth/XXXXXX" | pnpm wrangler secret put RPC_URL
echo "XXXXX" | pnpm wrangler secret put OPENSEA_API_KEY
```

Additionally, there is a hosted instance available at [worker.enstate.rs](https://worker.enstate.rs).

## Contributing

### Standalone Server

```sh
cd app && cargo run -p enstate
```

### Cloudflare Worker

```sh
cd worker && pnpm dev
```

For more information on running the worker locally, please see [running Cloudflare Workers locally](#run-the-worker-locally).

## ⛽ Gwei Name Service (`.gwei`)

> This fork also serves `.gwei` names of the [Gwei Name Service](https://github.com/lucadonnoh/gwei-names) (GNS),
> next to ENS. The hosted instances and the published docker image above are upstream's and do not: build from
> source to get `.gwei` support.

```sh
curl http://localhost:3000/n/gns.gwei                                     # .gwei name -> profile
curl http://localhost:3000/n/vitalik.eth                                  # ENS name -> profile
curl http://localhost:3000/a/0x881F5d0a21E8257C5a6a9137cDb09391df153889   # address -> primary name -> profile
curl "http://localhost:3000/bulk/u?queries[]=gns.gwei&queries[]=vitalik.eth"
```

The API is unchanged; which system answers depends on the name:

- **Names.** A name ending in `.gwei` is resolved on GNS. Every other name is resolved on ENS through the
  universal resolver, exactly as before. A `.gwei` name that cannot exist (an empty label) is a `400`.
- **Addresses.** Both systems are asked for the primary name of an address, at the same time. If the address
  has one in both, the `.gwei` name wins. `NAME_SERVICES_ORDER` changes that: `NAME_SERVICES_ORDER=ENS,GWEI`
  prefers the ENS name.

Both systems live on Ethereum mainnet, so they share `RPC_URL`.

### Configuration

On top of the variables enstate already had (`RPC_URL`, `REDIS_URL`, `UNIVERSAL_RESOLVER`, `OPENSEA_API_KEY`,
`PROFILE_RECORDS`, `MULTICOIN_CHAINS`, `MAX_BULK_SIZE`, `PROFILE_CACHE_TTL`, ...; see
[`.env.example`](./packages/app/.env.example)):

| Variable                  | Default                                      | Description                                                                                                                    |
| ------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `NAME_SERVICES_ORDER`     | `GWEI,ENS`                                   | Order the name services are preferred in when an address has a primary name in more than one. See below.                       |
| `GNS_CONTRACT`            | `0x9D51D507BC7264d4fE8Ad1cf7Fe191933A0a81d6` | The GNS `NameNFT` contract (same address on mainnet and Sepolia).                                                              |
| `MULTICALL_ADDRESS`       | `0xcA11bde05977b3631167028862bE2a173976CA11` | Multicall3, used to read a whole `.gwei` profile in one call.                                                                  |
| `WATCHER_ENABLED`         | `true`                                       | Set to `false` to not follow GNS events. `.gwei` entries then only change when they expire.                                    |
| `WS_RPC_URL`              | _unset_                                      | WebSocket JSON-RPC endpoint. If set, GNS events are received through a log subscription as they are mined.                     |
| `WATCHER_POLL_INTERVAL`   | `12`                                         | Seconds between two `eth_getLogs` polls.                                                                                       |
| `WATCHER_REORG_DEPTH`     | `12`                                         | Number of most recent blocks whose logs are re-read on every poll to notice reorganizations.                                   |
| `WATCHER_MAX_BLOCK_RANGE` | `1000`                                       | Largest block range to ask `eth_getLogs` for at once. Halved automatically when the node refuses a range.                      |

`NAME_SERVICES_ORDER` takes the values `ENS` and `GWEI`, in any case, either as a comma-separated list
(`GWEI,ENS`) or as a JSON array (`["ENS", "GWEI"]`). A service that is left out comes after the listed ones, so
`ENS` means the same as `ENS,GWEI`. Any other value stops the server at startup.

### Resolution

GNS is a single contract, `NameNFT`: it is the ERC-721, the registrar and the resolver, and the token id of a
name is its namehash. There is no registry to walk, no wildcard resolution and no CCIP-read.

- **Name → profile.** All records of a name (`addr`, `text`, `contenthash`, `addr(coinType)`) are read from
  `NameNFT` in a single `eth_call` through Multicall3, together with the expiry of the name and the number and
  timestamp of the block the answer comes from. A name that is not registered, has expired (the 90 day grace
  period counts as expired) or is a stale subdomain does not resolve and gives a `404`.
- **Address → name.** The `.gwei` primary name of an address is whatever `NameNFT.reverseResolve` says: the
  name the address picked, provided it is active and resolves back to the address.

`address` in a `.gwei` profile is `NameNFT.resolve`: the address set on the name, or else its owner. `resolver`
is the `NameNFT` contract. Names are lower-cased before they are hashed; this is not a full ENSIP-15
normalization.

### Caching

ENS entries are cached the way they always were: profiles under `n:<name>` and primary names under
`a:<address>`, for `PROFILE_CACHE_TTL` seconds.

`.gwei` entries live in their own namespace, profiles by namehash (`gwei:n:<node>`) and primary names by
address (`gwei:a:<address>`), both including negative answers. They are dropped as soon as something happens
on-chain that could change them; `PROFILE_CACHE_TTL` is only the upper bound for everything else (a watcher that
is not running or cannot reach its node, avatars that point at NFTs on other contracts). Nothing the watcher
does touches an ENS entry.

The primary name of an address is never cached as one combined answer: each system remembers its own, and the
order is applied on every request. A GNS event therefore changes the answer for an address immediately, whether
it was known by its `.gwei` name, by its ENS name or by none, and an ENS name is at most `PROFILE_CACHE_TTL`
seconds behind, as before.

### Events

A background task follows the logs of `NameNFT` and drops what they make stale. Every entry is registered under
the name it was derived from, and entries of subdomains additionally under each of their parents, so one event
finds all entries it affects without scanning:

| Event                                   | Dropped                                                                                                                   |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `TextChanged`, `ContenthashChanged`, `AddressChanged` | the profile of the name                                                                                     |
| `AddrChanged`, `Transfer`               | the profile of the name, and the primary name of every address whose primary name is that name                            |
| `NameRegistered`, `SubdomainRegistered`, `NameRenewed`, `Transfer` to the zero address (burn) | as above, plus the same for every name below it                     |
| `PrimaryNameSet`                        | the primary name of the address                                                                                           |

"The primary name of every address whose primary name is that name" covers both directions: addresses that had
the name and may have lost it, and addresses that point at it and may now have gained it.

Some changes emit no event of their own. They are covered as follows:

- **Expiry.** A name stops resolving the moment its `expiresAt` passes. No entry is ever stored for longer than
  the name (for subdomains: the top-level name above them) has left to live.
- **Stale subdomains.** Subdomains go stale when a parent is registered again or reclaimed. The parent's
  `NameRegistered` and burn `Transfer` drop everything cached below it.
- **Re-registration.** Registering an expired name again wipes its records and deletes the previous owner's
  primary name silently. The burn `Transfer` and `NameRegistered` it emits drop the profile and every primary
  name pointing at it.

### How logs are followed

- Logs are fetched with `eth_getLogs` every `WATCHER_POLL_INTERVAL` seconds. Each poll re-reads the last
  `WATCHER_REORG_DEPTH` blocks: logs that are new are applied, and logs that were applied but are no longer
  there (a reorganization) drop the same entries once more.
- With `WS_RPC_URL` set, a log subscription delivers the same logs as soon as they are mined. Polling keeps
  running underneath, so nothing is lost while the subscription is down.
- The last processed block is kept in Redis (`gwei:cursor`), so a restarted instance continues where it stopped.
  It never has to go back further than `PROFILE_CACHE_TTL` seconds worth of blocks, since older entries have
  expired anyway; the node behind `RPC_URL` has to be able to serve logs that far back.
- A lookup that was in flight while an event was applied could otherwise store what it read before the event.
  Every answer carries the block it was read at, and Redis refuses to store answers older than the last applied
  event (`gwei:fence`).

The cursor and the fence are block numbers: give every network its own Redis (database).

The watcher exposes `watcher_block`, `watcher_events_total` and `watcher_invalidated_total` on `/metrics`.

## Features

Here is a short summary of the features provided by the Enstate API including limitations.

### Avatar & Header Images

An additional `avatar` field at the top level of the ENSProfile object is provided. This field is a URL to the avatar image, with optional gateway rewrites for IPFS and IPNS hashes.

You can also directly access the avatar image of a user by using the `/i/{name}` and `/h/{name}` endpoints.

### Contenthash

Currently **limited implementation**. Only supports `ipfs`; names with an `ipns` or `swarm` contenthash (GNS
supports both) report it under `errors`.
TODO add support for `ipns`, `swarm`, `arweave`, `onion`, `onion3`, `skynet`

### Common Records

For each profile we look up the following records:
You can customize the records you want to query by adjusting the `PROFILE_RECORDS` environment variable.
Scoping down the size of this list can drastically improve the performance of your requests.

| Record Type                   | Description             |
| ----------------------------- | ----------------------- |
| `description`                 | Description             |
| `url`                         | URL to the profile      |
| `name`                        | Name of the profile     |
| `mail`                        | Email address           |
| `email`                       | Email address           |
| `avatar`                      | URL to the avatar       |
| `header`                      | URL to the header image |
| `display`                     | Display name            |
| `location`                    | Location                |
| `timezone`                    | Timezone                |
| `language`                    | Language                |
| `pronouns`                    | Pronouns                |
| `com.github`                  | GitHub username         |
| `org.matrix`                  | Matrix username         |
| `com.twitter`                 | Twitter username        |
| `com.discord`                 | Discord username        |
| `social.bsky`                 | Bsky username           |
| `io.keybase`                  | Keybase username        |
| `org.telegram`                | Telegram username       |
| `social.mastodon`             | Mastodon username       |
| `network.dm3.profile`         | DM3 profile             |
| `network.dm3.deliveryService` | DM3 delivery service    |

### Multichain Support

By default we query profiles for an vast array of chains.
You can customize the chains you want to query by adjusting the `MULTICOIN_CHAINS` environment variable.
Forcing it to only chains of interest can drastically improve the performance of your requests.
