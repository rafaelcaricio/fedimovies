# Mitra

Federated social network with smart contracts.

Built on [ActivityPub](https://activitypub.rocks/) protocol, self-hosted, lightweight.

Unique features enabled by blockchain integration:

- [Sign-in with Ethereum wallet](https://eips.ethereum.org/EIPS/eip-4361).
- Donations.
- Paid subscriptions.
- Proving membership with a token.
- Converting posts into NFTs.

Smart contracts repo: https://codeberg.org/silverpill/mitra-contracts

Frontend repo: https://codeberg.org/silverpill/mitra-web

Demo instance: https://mitra.social/ (invite-only)

Matrix chat: [#mitra:halogen.city](https://matrix.to/#/#mitra:halogen.city)

## Requirements

- Rust 1.54+ (when building from source)
- PostgreSQL 12+
- IPFS node (optional, see [guide](./docs/ipfs.md))
- Ethereum node (optional)

## Installation

### Building from source

Run:

```
cargo build --release --features production
```

This command will produce two binaries in `target/release` directory, `mitra` and `mitractl`.

Install PostgreSQL and create the database.

Create configuration file by copying `contrib/mitra_config.yaml` and configure the instance. Default config file path is `/etc/mitra/config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

Start Mitra:

```
./mitra
```

An HTTP server will be needed to handle HTTPS requests and serve the frontend. See the example of [nginx configuration file](./contrib/mitra.nginx).

Building instructions for `mitra-web` frontend can be found at https://codeberg.org/silverpill/mitra-web#project-setup.

To run Mitra as a systemd service, check out the [systemd unit file example](./contrib/mitra.service).

### Debian package

Download and install Mitra package:

```
dpkg -i mitra.deb
```

Install PostgreSQL and create the database. Open configuration file `/etc/mitra/config.yaml` and configure the instance.

Start Mitra:

```
systemctl start mitra
```

An HTTP server will be needed to handle HTTPS requests and serve the frontend. See the example of [nginx configuration file](./contrib/mitra.nginx).

## Development

### Create database

```
docker-compose up
```

Test connection:

```
psql -h localhost -p 55432 -U mitra mitra
```

### Run web service

Create config file, adjust settings if needed:

```
cp config.yaml.example config.yaml
```

Compile and run service:

```
cargo run
```

### Run CLI

```
cargo run --bin mitractl
```

### Run linter

```
cargo clippy
```

### Run tests

```
cargo test
```

### Build for production

```
cargo build --release
```

## Federation

See [FEDERATION.md](./FEDERATION.md)

## Client API

### Mastodon API

Most methods are similar to Mastodon API, but Mitra is not fully compatible.

[OpenAPI spec](./docs/openapi.yaml) (incomplete)

## CLI commands

Generate RSA private key:

```
mitractl generate-rsa-key
```

Generate invite code:

```
mitractl generate-invite-code
```

List generated invites:

```
mitractl list-invite-codes
```

Delete profile:

```
mitractl delete-profile -i 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete post:

```
mitractl delete-post -i 55a3005f-f293-4168-ab70-6ab09a879679
```

Remove remote posts and media older than 30 days:

```
mitractl delete-extraneous-posts -d 30
```

Delete attachments that doesn't belong to any post:

```
mitractl delete-unused-attachments -d 5
```

Generate ethereum address:

```
mitractl generate-ethereum-address
```

## License

[AGPL-3.0](./LICENSE)

## Support

Monero: 8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p
