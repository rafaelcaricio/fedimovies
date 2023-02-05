# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Replace post attachments and other related objects when processing `Update(Note)` activity.
- Append attachment URL to post content if attachment size exceeds limit.
- Added `/api/v1/custom_emojis` endpoint.
- Added `limits` parameter group to configuration.
- Made file size limit adjustable with `limits.media.file_size_limit` configuration option.
- Added `limits.posts.character_limit` configuration parameter (replaces `post_character_limit`).
- Implemented automatic pruning of remote posts and empty profiles (disabled by default).

### Changed

- Use proof suites with prefix `Mitra`.
- Added `https://w3id.org/security/data-integrity/v1` to JSON-LD context.
- Return `202 Accepted` when activity is accepted by inbox endpoint.
- Ignore forwarded `Like` activities.
- Set 10 minute timeout on background job that processes incoming activities.
- Use "warn" log level for delivery errors.
- Don't allow read-only users to manage subscriptions.

### Deprecated

- Deprecated `post_character_limit` configuration option.

### Fixed

- Change max body size in nginx example config to match app limit.
- Don't create invoice if recipient can't accept subscription payments.
- Ignore `Announce(Delete)` activities.

## [1.12.0] - 2023-01-26

### Added

- Added `approval_required` and `invites_enabled` flags to `/api/v1/instance` endpoint response.
- Added `registration.type` configuration option (replaces `registrations_open`).
- Implemented roles & permissions.
- Added "read-only user" role.
- Added configuration option for automatic assigning of "read-only user" role after registration.
- Added `set-role` command.

### Changed

- Don't retry activity if fetcher recursion limit has been reached.

### Deprecated

- `registrations_open` configuration option.

### Removed

- Dropped support for `blockchain` configuration parameter.

### Fixed

- Added missing `<link rel="self">` element to Atom feeds.
- Added missing `<link rel="alternate">` element to Atom feed entries.

## [1.11.0] - 2023-01-23

### Added

- Save sizes of media attachments and other files to database.
- Added `import-emoji` command.
- Added support for emoji shortcodes.
- Allowed custom emojis with `image/apng` media type.

### Changed

- Make `delete-emoji` command accept emoji name and hostname instead of ID.
- Replaced client-side tag URLs with collection IDs.

### Security

- Validate emoji name before saving.

## [1.10.0] - 2023-01-18

### Added

- Added `/api/v1/settings/move_followers` API endpoint (replaces `/api/v1/accounts/move_followers`).
- Added `/api/v1/settings/import_follows` API endpoint.
- Validation of Monero subscription payout address.
- Accept webfinger requests where `resource` is actor ID.
- Adeed support for `as:Public` and `Public` audience identifiers.
- Displaying custom emojis.

### Changed

- Save downloaded media as "unknown" if its media type is not supported.
- Use `mediaType` property value to determine file extension when saving downloaded media.
- Added `mediaType` property to images in actor object.
- Prevent `delete-extraneous-posts` command from deleting post if there's a recent reply or repost.
- Changed max actor image size to 5 MB.

### Removed

- `/api/v1/accounts/move_followers` API endpoint.

### Fixed

- Don't ignore `Delete(Person)` verification errors if database error subtype is not `NotFound`.
- Don't stop activity processing on invalid local mentions.
- Accept actor objects where `attachment` property value is not an array.
- Don't download HTML pages attached by GNU Social.
- Ignore `Like()` activity if local post doesn't exist.
- Fixed `.well-known` paths returning `400 Bad Request` errors.

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
