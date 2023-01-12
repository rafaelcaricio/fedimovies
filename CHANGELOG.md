# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Added `/api/v1/settings/move_followers` API endpoint (replaces `/api/v1/accounts/move_followers`).
- Added `/api/v1/settings/import_follows` API endpoint.
- Validation of Monero subscription payout address.
- Accept webfinger requests where `resource` is actor ID.
- Adeed support for `as:Public` audience identifier.

### Removed

- `/api/v1/accounts/move_followers` API endpoint.

### Fixed

- Don't ignore `Delete(Person)` verification errors if database error subtype is not `NotFound`.
- Don't stop activity processing on invalid local mentions.

## [1.9.0] - 2023-01-08

### Added

- Added `/api/v1/accounts/lookup` Mastodon API endpoint.
- Implemented activity delivery queue.
- Started to keep track of unreachable actors.
- Added `configuration` object to response of `/api/v1/instance` endpoint.
- Save media types of uploaded avatar and banner images.
- Support for `MitraJcsRsaSignature2022` and `MitraJcsEip191Signature2022` signature suites.

### Changed

- Updated installation instructions, default mitra config and recommended nginx config.
- Limited the number of requests made during the processing of a thread.
- Limited the number of media files that can be attached to a post.

### Deprecated

- Deprecated `post_character_limit` property in `/api/v1/instance` response.
- Avatar and banner uploads without media type via `/api/v1/accounts/update_credentials`.
- `JcsRsaSignature2022` and `JcsEip191Signature2022` signature suites.

### Removed

- Removed ability to upload non-images using `/api/v1/media` endpoint.

### Fixed

- Fixed post and profile page redirections.
- Fixed federation with GNU Social.
