# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Added `/api/v1/accounts/lookup` Mastodon API endpoint.
- Implemented activity delivery queue.
- Started to keep track of unreachable actors.

### Changed

- Updated installation instructions, default mitra config and recommended nginx config.
- Limited the number of requests made during the processing of a thread.
- Limited the number of media files that can be attached to a post.

### Removed

- Removed ability to upload non-images using `/api/v1/media` endpoint.

### Fixed

- Fixed post and profile page redirections.
- Fixed federation with GNU Social.
