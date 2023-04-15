# Mitra

Federated micro-blogging platform.

Built on [ActivityPub](https://www.w3.org/TR/activitypub/) protocol, self-hosted, lightweight. Part of the [Fediverse](https://en.wikipedia.org/wiki/Fediverse).

Features:

- Micro-blogging service (includes support for quote posts, custom emojis and more).
- Mastodon API.
- Content subscription service. Subscriptions provide a way to receive monthly payments from subscribers and to publish private content made exclusively for them.
  - Supported payment methods: [Monero](https://www.getmonero.org/get-started/what-is-monero/) and [ERC-20](https://ethereum.org/en/developers/docs/standards/tokens/erc-20/) tokens (on Ethereum and other EVM-compatible blockchains).
- [Sign-in with a wallet](https://eips.ethereum.org/EIPS/eip-4361).
- Donation buttons.
- Account migrations (from one server to another). Identity can be detached from the server.
- Federation over Tor.

Follow: [@mitra@mitra.social](https://mitra.social/@mitra)

Matrix chat: [#mitra:halogen.city](https://matrix.to/#/#mitra:halogen.city)

## Instances

- [FediList](http://demo.fedilist.com/instance?software=mitra)
- [Fediverse Observer](https://mitra.fediverse.observer/list)

Demo instance: https://public.mitra.social/ ([invite-only](https://public.mitra.social/about))

## Code

Server: https://codeberg.org/silverpill/mitra (this repo)

Web client: https://codeberg.org/silverpill/mitra-web

Ethereum contracts: https://codeberg.org/silverpill/mitra-contracts

## Requirements

- Rust 1.57+ (when building from source)
- PostgreSQL 12+

Optional:

- Monero node and Monero wallet service
- Ethereum node
- IPFS node (see [guide](./docs/ipfs.md))

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

Put any static files into the directory specified in configuration file. Building instructions for `mitra-web` frontend can be found at https://codeberg.org/silverpill/mitra-web#project-setup.

Start Mitra:

```shell
./mitra
```

An HTTP server will be needed to handle HTTPS requests. See the example of [nginx configuration file](./contrib/mitra.nginx).

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

An HTTP server will be needed to handle HTTPS requests. See the example of [nginx configuration file](./contrib/mitra.nginx).

### Docker image

This image is maintained by the community and has not been verified:

https://hub.docker.com/r/bleakfuture0/mitra

## Configuration

### Tor federation

See [guide](./docs/onion.md).

### Blockchain integrations

- [Monero](./docs/monero.md)
- [Ethereum](./docs/ethereum.md)

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
