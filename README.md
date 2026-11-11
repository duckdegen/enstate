<img src=".github/banner.png#1" alt="enstate.rs" />

> This is a fork of [v3xlabs/enstate](https://github.com/v3xlabs/enstate) that serves `.gwei` names of the
> [Gwei Name Service](https://github.com/lucadonnoh/gwei-names) (GNS) instead of ENS names, and keeps its cache
> in sync with the chain by following the events of the GNS contract.

## 📌 What it does

enstate is an HTTP API that turns a `.gwei` name (or an address with a primary `.gwei` name) into a profile:
address, text records, contenthash, multichain addresses, avatar and header. Profiles are cached in Redis.

```sh
curl http://localhost:3000/n/gns.gwei                                     # name -> profile
curl http://localhost:3000/a/0xC04689227Fa24785609B1174698DBe481437f1A3   # address -> primary name -> profile
curl http://localhost:3000/u/gns.gwei                                     # either
curl "http://localhost:3000/bulk/u?queries[]=gns.gwei&queries[]=donnoh.gwei"
```

The API is the one enstate has always had (`/n`, `/a`, `/u`, `/i`, `/h`, `/bulk/*`, `/sse/*`), and the OpenAPI
specification is served at `/docs`.

Only `.gwei` names are served. Any other name (`luc.eth`, `example.com`) is rejected with a `400`; nothing is
looked up on ENS anymore. Names are lower-cased before they are hashed; like upstream enstate this is not a
full ENSIP-15 normalization.

## 🚀 Getting Started

You need a Redis instance and a JSON-RPC endpoint for the chain GNS lives on (Ethereum mainnet by default).

```sh
cd packages/app
cp .env.example .env    # then fill in RPC_URL etc.
docker compose up -d redis
cargo run
```

### Configuration

| Variable                  | Default                                      | Description                                                                                                                                             |
| ------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PORT`                    | `3000`                                       | Port to listen on.                                                                                                                                      |
| `RPC_URL`                 | _required_                                   | One or more comma-separated HTTP(S) JSON-RPC endpoints. Used for lookups and for polling logs.                                                          |
| `WS_RPC_URL`              | _unset_                                      | WebSocket JSON-RPC endpoint. If set, GNS events are received through a log subscription as they are mined instead of at the next poll.                  |
| `REDIS_URL`               | _unset_                                      | Redis to cache in. Without it nothing is cached and the watcher does not run.                                                                           |
| `GNS_CONTRACT`            | `0x9D51D507BC7264d4fE8Ad1cf7Fe191933A0a81d6` | The GNS `NameNFT` contract (same address on mainnet and Sepolia).                                                                                       |
| `MULTICALL_ADDRESS`       | `0xcA11bde05977b3631167028862bE2a173976CA11` | Multicall3, used to read a whole profile in one call.                                                                                                   |
| `PROFILE_CACHE_TTL`       | `600`                                        | Longest time in seconds a cache entry may live. See [Caching](#-caching).                                                                               |
| `WATCHER_ENABLED`         | `true`                                       | Set to `false` to not follow GNS events. Entries then only change when they expire.                                                                     |
| `WATCHER_POLL_INTERVAL`   | `12`                                         | Seconds between two `eth_getLogs` polls.                                                                                                                |
| `WATCHER_REORG_DEPTH`     | `12`                                         | Number of most recent blocks whose logs are re-read on every poll to notice reorganizations.                                                            |
| `WATCHER_MAX_BLOCK_RANGE` | `1000`                                       | Largest block range to ask `eth_getLogs` for at once. Halved automatically when the node refuses a range.                                               |
| `PROFILE_RECORDS`         | see [Common Records](#common-records)        | Comma-separated text records to include in a profile.                                                                                                   |
| `MULTICOIN_CHAINS`        | see [Multichain Support](#multichain-support) | Comma-separated coin types to include in a profile.                                                                                                     |
| `MAX_BULK_SIZE`           | `10`                                         | Largest number of names or addresses per bulk request.                                                                                                  |
| `OPENSEA_API_KEY`         | _required_                                   | Used to resolve NFT avatars.                                                                                                                            |
| `IPFS_GATEWAY`            | `https://ipfs.io/ipfs/`                      | Gateway `ipfs://` avatars and headers are rewritten to.                                                                                                 |
| `AR_GATEWAY`              | `https://arweave.net/`                       | Gateway `ar://` avatars and headers are rewritten to.                                                                                                   |

## 🔎 Resolution

GNS is a single contract, `NameNFT`: it is the ERC-721, the registrar and the resolver, and the token id of a
name is its namehash. There is no registry to walk, no wildcard resolution and no CCIP-read.

- **Name → profile.** All records of a name (`addr`, `text`, `contenthash`, `addr(coinType)`) are read from
  `NameNFT` in a single `eth_call` through Multicall3, together with the expiry of the name and the number and
  timestamp of the block the answer comes from. A name that is not registered, has expired (the 90 day grace
  period counts as expired) or is a stale subdomain does not resolve and gives a `404`.
- **Address → profile.** The primary name of an address is whatever `NameNFT.reverseResolve` says: the name the
  address picked, provided it is active and resolves back to the address. The profile of that name follows.

`address` in a profile is `NameNFT.resolve`: the address set on the name, or else its owner. `resolver` is always
the `NameNFT` contract.

## 🗄 Caching

Profiles are cached by namehash (`gwei:n:<node>`), primary names by address (`gwei:a:<address>`), both
including negative answers. An entry is dropped as soon as something happens on-chain that could change it;
`PROFILE_CACHE_TTL` is only the upper bound for everything else (a watcher that is not running or cannot reach
its node, avatars that point at NFTs on other contracts).

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

An additional `avatar` field at the top level of the profile object is provided. This field is a URL to the avatar image, with optional gateway rewrites for IPFS and IPNS hashes.

You can also directly access the avatar image of a user by using the `/i/{name}` and `/h/{name}` endpoints.

### Contenthash

Currently **limited implementation**. Only supports `ipfs`; GNS names with an `ipns` or `swarm` contenthash
report it under `errors`.
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
