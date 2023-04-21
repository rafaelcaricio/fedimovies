# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Added support for content warnings.
- Support integrity proofs with `DataIntegrityProof` type.

### Changed

- Ignore errors when importing activities from outbox.
- Make activity limit in outbox fetcher adjustable.
- Updated actix to latest version. MSRV changed to 1.57.

### Fixed

- Make `/api/v1/accounts/{account_id}/follow` work with form-data.

## [1.21.0] - 2023-04-12

### Added

- Added `create-user` command.
- Added `read-outbox` command.

### Changed

- Added emoji count check to profile data validator.
- Check mention and link counts when creating post.
- Re-fetch object if `attributedTo` value doesn't match `actor` of `Create` activity.
- Added actor validation to `Update(Note)` and `Undo(Follow)` handlers.

### Fixed

- Fixed database query error in `Create` activity handler.

## [1.20.0] - 2023-04-07

### Added

- Support calling `/api/v1/accounts/search` with `resolve` parameter.
- Created `/api/v1/accounts/aliases/all` API endpoint.
- Created API endpoint for adding aliases.
- Populate `alsoKnownAs` property on actor object with declared aliases.
- Support account migration from Mastodon.
- Created API endpoint for managing client configurations.
- Reject unsolicited public posts.

### Changed

- Increase maximum number of custom emojis per post to 50.
- Validate actor aliases before saving into database.
- Process incoming `Move()` activities in background.
- Allow custom emojis with `image/webp` media type.
- Increase object ID size limit to 2000 chars.
- Increase fetcher timeout to 15 seconds when processing search queries.

### Fixed

- Added missing `CHECK` constraints to database tables.
- Validate object ID length before saving post to database.
- Validate emoji name length before saving to database.

## [1.19.1] - 2023-03-31

### Changed

- Limit number of mentions and links in remote posts.

### Fixed

- Process queued background jobs before re-trying stalled.
- Remove activity from queue if handler times out.
- Order attachments by creation date when new post is created.

## [1.19.0] - 2023-03-30

### Added

- Added `prune-remote-emojis` command.
- Prune remote emojis in background.
- Added `limits.media.emoji_size_limit` configuration parameter.
- Added `federation.fetcher_timeout` and `federation.deliverer_timeout` configuration parameters.

### Changed

- Allow emoji names containing hyphens.
- Increased remote emoji size limit to 500 kB.
- Set fetcher timeout to 5 seconds when processing search queries.

### Fixed

- Fixed error in emoji update SQL query.
- Restart stalled background jobs.
- Order attachments by creation date.
- Don't reopen monero wallet on each subscription monitor run.

### Security

- Updated markdown parser to latest version.

## [1.18.0] - 2023-03-21

### Added

- Added `fep-e232` feature flag (disabled by default).
- Added `account_index` parameter to Monero configuration.
- Added `/api/v1/instance/peers` API endpoint.
- Added `federation.enabled` configuration parameter that can be used to disable federation.

### Changed

- Documented valid role names for `set-role` command.
- Granted `delete_any_post` and `delete_any_profile` permissions to admin role.
- Updated profile page URL template to match mitra-web.

### Fixed

- Make webclient-to-object redirects work for remote profiles and posts.
- Added webclient redirection rule for `/@username` routes.
- Don't allow migration if user doesn't have identity proofs.

## [1.17.0] - 2023-03-15

### Added

- Enabled audio and video uploads.
- Added `audio/ogg` and `audio/x-wav` to the list of supported media types.

### Changed

- Save latest ethereum block number to database instead of file.
- Removed hardcoded upload size limit.

### Deprecated

- Reading ethereum block number from `current_block` file.

### Removed

- Disabled post tokenization (can be re-enabled with `ethereum-extras` feature).
- Removed ability to switch from Ethereum devnet to another chain without resetting subscriptions.

### Fixed

- Allow `!` after hashtags and mentions.
- Ignore emojis with non-unique names in remote posts.

## [1.16.0] - 2023-03-08

### Added

- Allow to add notes to generated invite codes.
- Added `registration.default_role` configuration option.
- Save emojis attached to actor objects.
- Added `emojis` field to Mastodon API Account entity.
- Support audio attachments.
- Added CLI command for viewing unreachable actors.
- Implemented NodeInfo 2.1.
- Added `federation.onion_proxy_url` configuration parameter (enables proxy for requests to `.onion` domains).

### Changed

- Use .jpg extension for files with image/jpeg media type.

### Deprecated

- Deprecated `default_role_read_only_user` configuration option (replaced by `registration.default_role`).

## [1.15.0] - 2023-02-27

### Added

- Set fetcher timeout to 3 minutes.
- Set deliverer timeout to 30 seconds.
- Added `federation` parameter group to configuration.
- Add empty `spoiler_text` property to Mastodon API Status object.
- Added `error` and `error_description` fields to Mastodon API error responses.
- Store information about failed activity deliveries in database.
- Added `/api/v1/accounts/{account_id}/aliases` API endpoint.

### Changed

- Put activities generated by CLI commands in a queue instead of immediately sending them.
- Changed path of user's Atom feed to `/feeds/users/{username}`.
- Increase number of delivery attempts and increase intervals between them.

### Deprecated

- Deprecated `proxy_url` configuration parameter (replaced by `federation.proxy_url`).
- Deprecated Atom feeds at `/feeds/{username}`.
- Deprecated `message` field in Mastodon API error response.

### Fixed

- Prevent `delete-extraneous-posts` command from removing locally-linked posts.
- Make webfinger response compatible with GNU Social account lookup.
- Prefer `Group` actor when doing webfinger query on Lemmy server.
- Fetch missing profiles before doing follower migration.
- Follow FEP-e232 links when importing post.

## [1.14.0] - 2023-02-22

### Added

- Added `/api/v1/apps` endpoint.
- Added OAuth authorization page.
- Support `authorization_code` OAuth grant type.
- Documented `http_cors_allowlist` configuration parameter.
- Added `/api/v1/statuses/{status_id}/thread` API endpoint (replaces `/api/v1/statuses/{status_id}/context`).
- Accept webfinger requests where `resource` is instance actor ID.
- Added `proxy_set_header X-Forwarded-Proto $scheme;` directive to nginx config example.
- Add `Content-Security-Policy` and `X-Content-Type-Options` headers to all responses.

### Changed

- Allow `instance_uri` configuration value to contain URI scheme.
- Changed `/api/v1/statuses/{status_id}/context` response format to match Mastodon API.
- Changed status code of `/api/v1/statuses` response to 200 to match Mastodon API.
- Removed `add_header` directives for `Content-Security-Policy` and `X-Content-Type-Options` headers from nginx config example.

### Deprecated

- Deprecated protocol guessing on incoming requests (use `X-Forwarded-Proto` header).

### Fixed

- Fixed actor object JSON-LD validation errors.
- Fixed activity JSON-LD validation errors.
- Make media URLs in Mastodon API responses relative to current origin.

## [1.13.1] - 2023-02-09

### Fixed

- Fixed permission error on subscription settings update.

## [1.13.0] - 2023-02-06

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
