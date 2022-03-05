# Mitra

Federated social network with smart contracts.

Built on [ActivityPub](https://activitypub.rocks/) protocol, self-hosted, lightweight.

Unique features enabled by blockchain integration:

- Sign-in with Ethereum.
- Proving membership with a token.
- Paid subscriptions.
- Converting posts into NFTs.

Smart contracts repo: https://codeberg.org/silverpill/mitra-contracts

Frontend repo: https://codeberg.org/silverpill/mitra-web

Matrix chat: [#mitra:halogen.city](https://matrix.to/#/#mitra:halogen.city)

Demo instance: https://mitra.social/ (invite-only)

## Requirements

- Rust 1.51+
- PostgreSQL 10.2+
- IPFS node (optional, see [guide](./docs/ipfs.md))
- Ethereum node (optional)

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

Generate instance key:

```
cargo run --bin mitractl generate-rsa-key
```

Create config file, set `instance_rsa_key`, adjust other settings if needed:

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

The following activities are supported:

- Accept(Follow)
- Reject(Follow)
- Undo(Follow)
- Create(Note)
- Delete(Note)
- Like(Note)
- Undo(Like)
- Announce(Note)
- Undo(Announce)
- Follow(Person)
- Update(Person)

And these additional standards:

- [Http Signatures](https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures)
- [NodeInfo](https://nodeinfo.diaspora.software/)
- [WebFinger](https://webfinger.net/)

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

Generate ethereum address:

```
mitractl generate-ethereum-address
```

## License

[AGPL-3.0](./LICENSE)
