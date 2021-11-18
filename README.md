# Mitra

Federated social network with smart contracts.

- Built on [ActivityPub](https://activitypub.rocks/) protocol.
- Lightweight.
- Sign-in with Ethereum.
- Proving membership with a token.
- Converting posts into NFTs.
- More crypto features in the future.

Demo instance: https://mitra.social/ (invite-only)

Smart contracts repo: https://codeberg.org/silverpill/mitra-contracts

## Requirements

- Rust 1.51+
- Postgresql
- IPFS node (optional)
- Ethereum node (optional)

## Development

### Create database

```
docker-compose up
```

Test connection:

```
psql -h localhost -p 5432 -U mitra mitra
```

### Run web service

Create config file:

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
- Create(Note)
- Delete(Tombstone)
- Like(Note)
- Follow(Person)
- Undo(Follow)
- Update(Person)

And these additional standards:

- [Http Signatures](https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures)
- [NodeInfo](https://nodeinfo.diaspora.software/)
- [WebFinger](https://webfinger.net/)

## Client API

### Mastodon API

Most methods are similar to Mastodon API:

```
POST /api/v1/accounts
GET /api/v1/accounts/{account_id}
GET /api/v1/accounts/verify_credentials
PATCH /api/v1/accounts/update_credentials
GET /api/v1/accounts/relationships
POST /api/v1/accounts/{account_id}/follow
POST /api/v1/accounts/{account_id}/unfollow
GET /api/v1/directory
GET /api/v1/instance
GET /api/v1/markers
POST /api/v1/markers
POST /api/v1/media
GET /api/v1/notifications
GET /api/v2/search
POST /api/v1/statuses
GET /api/v1/statuses/{status_id}
GET /api/v1/statuses/{status_id}/context
POST /api/v1/statuses/{status_id}/favourite
POST /api/v1/statuses/{status_id}/unfavourite
GET /api/v1/timelines/home
```

Additional methods:

```
POST /api/v1/statuses/{status_id}/make_permanent
GET /api/v1/statuses/{status_id}/signature
```

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
