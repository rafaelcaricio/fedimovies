# ActivityPub federation in Mitra

Mitra largely follows the [ActivityPub](https://www.w3.org/TR/activitypub/) server-to-server specification but it makes uses of some non-standard extensions, some of which are required for interacting with it at all.

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

