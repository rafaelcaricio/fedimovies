# FediMovies
[![status-badge](https://ci.caric.io/api/badges/FediMovies/fedimovies/status.svg)](https://ci.caric.io/FediMovies/fedimovies)

Lively federated movies reviews platform.

Built on [ActivityPub](https://www.w3.org/TR/activitypub/) protocol, self-hosted, lightweight. Part of the [Fediverse](https://en.wikipedia.org/wiki/Fediverse).

Features:

- Micro-blogging service (includes support for quote posts, custom emojis and more).
- Mastodon API.
- Account migrations (from one server to another). Identity can be detached from the server.
- Federation over Tor.

## Instances

- [FediList](http://demo.fedilist.com/instance?software=fedimovies)
- [Fediverse Observer](https://fedimovies.fediverse.observer/list)

Demo instance: https://nullpointer.social/ ([invite-only](https://nullpointer.social/about))

## Code

Server: https://code.caric.io/reef/reef (this repo)

Web client: 


## Requirements

- Rust 1.57+ (when building from source)
- PostgreSQL 12+

Optional:

- IPFS node (see [guide](./docs/ipfs.md))

## Installation

### Building from source

Run:

```shell
cargo build --release --features production
```

This command will produce two binaries in `target/release` directory, `fedimovies` and `fedimoviesctl`.

Install PostgreSQL and create the database:

```sql
CREATE USER fedimovies WITH PASSWORD 'fedimovies';
CREATE DATABASE fedimovies OWNER fedimovies;
```

Create configuration file by copying `contrib/fedimovies_config.yaml` and configure the instance. Default config file path is `/etc/fedimovies/config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

Put any static files into the directory specified in configuration file. Building instructions for `fedimovies-web` frontend can be found at https://code.caric.io/FediMovies/fedimovies#project-setup.

Start Fedimovies:

```shell
./fedimovies
```

An HTTP server will be needed to handle HTTPS requests. See the example of [nginx configuration file](./contrib/fedimovies.nginx).

To run Fedimovies as a systemd service, check out the [systemd unit file example](./contrib/fedimovies.service).

### Debian package

Download and install Fedimovies package:

```shell
dpkg -i fedimovies.deb
```

Install PostgreSQL and create the database:

```sql
CREATE USER fedimovies WITH PASSWORD 'fedimovies';
CREATE DATABASE fedimovies OWNER fedimovies;
```

Open configuration file `/etc/fedimovies/config.yaml` and configure the instance.

Start Fedimovies:

```shell
systemctl start fedimovies
```

An HTTP server will be needed to handle HTTPS requests. See the example of [nginx configuration file](./contrib/fedimovies.nginx).

### Tor federation

See [guide](./docs/onion.md).

## Development

See [CONTRIBUTING.md](./CONTRIBUTING.md)

### Start database server

```shell
docker-compose up -d
```

Test connection:

```shell
psql -h localhost -p 55432 -U fedimovies fedimovies
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
cargo run --bin fedimoviesctl
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

Most methods are similar to Mastodon API, but Fedimovies is not fully compatible.

[OpenAPI spec](./docs/openapi.yaml)

## CLI

`fedimoviesctl` is a command-line tool for performing instance maintenance.

[Documentation](./docs/fedimoviesctl.md)

## License

[AGPL-3.0](./LICENSE)
