# ActivityPub federation in Mitra

Mitra largely follows the [ActivityPub](https://www.w3.org/TR/activitypub/) server-to-server specification but it makes uses of some non-standard extensions, some of which are required for interacting with it.

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
- Update(Note)
- Follow(Person)
- Update(Person)
- Move(Person)
- Delete(Person)
- Add(Person)
- Remove(Person)

And these additional standards:

- [Http Signatures](https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures)
- [NodeInfo](https://nodeinfo.diaspora.software/)
- [WebFinger](https://webfinger.net/)

Activities are implemented in way that is compatible with Pleroma, Mastodon and other popular ActivityPub servers.

## Supported FEPs

- [FEP-f1d5](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-f1d5.md)
- [FEP-e232](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-e232.md)

## Profile extensions

### Cryptocurrency addresses

Cryptocurrency addresses are represented as `PropertyValue` attachments where `name` attribute is a currency symbol prefixed with `$`:

```json
{
  "name": "$XMR",
  "type": "PropertyValue",
  "value": "8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p"
}
```

### Identity proofs

Identity proofs are represented as attachments of `IdentityProof` type:

```json
{
  "name": "<did>",
  "type": "IdentityProof",
  "signatureAlgorithm": "<proof-type>",
  "signatureValue": "<proof>"
}
```

Supported proof types:

- EIP-191 (Ethereum personal signatures)
- [Minisign](https://jedisct1.github.io/minisign/)

## Subscription events

Local actor profiles have `subscribers` property which points to the collection of actor's paid subscribers.

The `Add` activity is used to notify the subscriber about successful subscription payment. Upon receipt of this activity, the receiving server should add specified `object` to actors's `subscribers` collection (specified in `target` property):

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "actor": "https://example.com/users/alice",
  "id": "https://example.com/activities/00000000-0000-0000-0000-000000000001",
  "object": "https://example.com/users/bob",
  "target": "https://example.com/users/alice/collections/subscribers",
  "to": [
    "https://example.com/users/bob"
  ],
  "type": "Add"
}
```

The `Remove` activity is used to notify the subscriber about expired subscription. Upon receipt of this activity, the receiving server should remove specified `object` from actors's `subscribers` collection (specified in `target` property):

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "actor": "https://example.com/users/alice",
  "id": "https://example.com/activities/00000000-0000-0000-0000-000000000002",
  "object": "https://example.com/users/bob",
  "target": "https://example.com/users/alice/collections/subscribers",
  "to": [
    "https://example.com/users/bob"
  ],
  "type": "Remove"
}
```
