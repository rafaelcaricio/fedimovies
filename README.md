# Mitra

Federated social network with smart contracts.

- Built on [ActivityPub](https://activitypub.rocks/) protocol.
- Lightweight.
- Sign-in with Ethereum.
- Converting posts into NFTs.
- More crypto features in the future.

**WIP: Mitra is not ready for production yet.**

Demo instance: https://test.mitra.social/ (invite-only)

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

### Build for production

```
cargo build --release
```

## API

### Mastodon API

Endpoints are similar to Mastodon API:

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
POST /api/v1/media
GET /api/v2/search
POST /api/v1/statuses
GET /api/v1/statuses/{status_id}
GET /api/v1/statuses/{status_id}/context
GET /api/v1/timelines/home
```

Extra APIs:

```
POST /api/v1/statuses/{status_id}/make_permanent
GET /api/v1/statuses/{status_id}/signature
```

## CLI commands

Delete profile:

```
mitractl delete-profile -i 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete post:

```
mitractl delete-post -i 55a3005f-f293-4168-ab70-6ab09a879679
```

Generate invite code:

```
mitractl generate-invite-code
```

List generated invites:

```
mitractl list-invite-codes
```

Generate ethereum address:

```
mitractl generate-ethereum-address
```
