# Mitra

Federated micro-blogging platform and content subscription service.

Built on [ActivityPub](https://www.w3.org/TR/activitypub/) protocol, self-hosted, lightweight. Part of the [Fediverse](https://en.wikipedia.org/wiki/Fediverse).

Subscriptions provide a way to receive monthly payments from subscribers and to publish private content made exclusively for them.

Supported payment methods:

- [Monero](https://www.getmonero.org/get-started/what-is-monero/).
- [ERC-20](https://ethereum.org/en/developers/docs/standards/tokens/erc-20/) tokens (on Ethereum and other EVM-compatible blockchains).

Other features:

- [Sign-in with a wallet](https://eips.ethereum.org/EIPS/eip-4361).
- Account migrations (from one server to another).
- Donation buttons.
- Token-gated registration (can be used to verify membership in some group or to stop bots).
- Converting posts into NFTs.
- Saving posts to IPFS.

Demo instance: https://public.mitra.social/ ([invite-only](https://public.mitra.social/about))

Network stats: https://the-federation.info/mitra

Matrix chat: [#mitra:halogen.city](https://matrix.to/#/#mitra:halogen.city)

## Code

Server: https://codeberg.org/silverpill/mitra (this repo)

Web client: https://codeberg.org/silverpill/mitra-web

Ethereum contracts: https://codeberg.org/silverpill/mitra-contracts

## Requirements

- Rust 1.54+ (when building from source)
- PostgreSQL 12+
- IPFS node (optional, see [guide](./docs/ipfs.md))
- Ethereum node (optional)
- Monero node and Monero wallet (optional)

## Installation

### Building from source

Run:

```shell
cargo build --release --features production
```

This command will produce two binaries in `target/release` directory, `mitra` and `mitractl`.

Install PostgreSQL and create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra;
```

Create configuration file by copying `contrib/mitra_config.yaml` and configure the instance. Default config file path is `/etc/mitra/config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

Start Mitra:

```shell
./mitra
```

An HTTP server will be needed to handle HTTPS requests and serve the frontend. See the example of [nginx configuration file](./contrib/mitra.nginx).

Building instructions for `mitra-web` frontend can be found at https://codeberg.org/silverpill/mitra-web#project-setup.

To run Mitra as a systemd service, check out the [systemd unit file example](./contrib/mitra.service).

### Debian package

Download and install Mitra package:

```shell
dpkg -i mitra.deb
```

Install PostgreSQL and create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra;
```

Open configuration file `/etc/mitra/config.yaml` and configure the instance.

Start Mitra:

```shell
systemctl start mitra
```

An HTTP server will be needed to handle HTTPS requests and serve the frontend. See the example of [nginx configuration file](./contrib/mitra.nginx).

### Monero

Install Monero node or choose a [public one](https://monero.fail/).

Configure and start [monero-wallet-rpc](https://monerodocs.org/interacting/monero-wallet-rpc-reference/) daemon. Add `disable-rpc-login=1` to your `monero-wallet-rpc` config (currently RPC auth is not supported in Mitra).

Create a wallet for your instance.

Add blockchain configuration to `blockchains` array in your configuration file.

### Ethereum

Install Ethereum client or choose a JSON-RPC API provider.

Deploy contracts on the blockchain. Instructions can be found at https://codeberg.org/silverpill/mitra-contracts.

Add blockchain configuration to `blockchains` array in your configuration file.

## Development

See [CONTRIBUTING.md](./CONTRIBUTING.md)

### Start database server

```shell
docker-compose up -d
```

Test connection:

```shell
psql -h localhost -p 55432 -U mitra mitra
```

### Start Monero node and wallet server

(this step is optional)

```shell
docker-compose --profile monero up -d
```

### Run web service

Create config file, adjust settings if needed:

```shell
cp config.yaml.example config.yaml
```

Compile and run service:

```shell
cargo run
```

### Run CLI

```shell
cargo run --bin mitractl
```

### Run linter

```shell
cargo clippy
```

### Run tests

```shell
cargo test
```

## Federation

See [FEDERATION.md](./FEDERATION.md)

## Client API

Most methods are similar to Mastodon API, but Mitra is not fully compatible.

[OpenAPI spec](./docs/openapi.yaml)

## CLI

`mitractl` is a command-line tool for performing instance maintenance.

[Documentation](./docs/mitractl.md)

## License

[AGPL-3.0](./LICENSE)

## Support

Monero: 8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p
