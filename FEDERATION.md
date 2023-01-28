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

Supported FEPs:

- [FEP-f1d5: NodeInfo in Fediverse Software](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-f1d5.md)
- [FEP-e232: Object Links](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-e232.md)
- [FEP-8b32: Object Integrity Proofs](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-8b32.md)

## Object integrity proofs

All outgoing activities are signed with actor's key in accordance with [FEP-8b32](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-8b32.md) document.

Example:

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams",
    "https://w3id.org/security/data-integrity/v1"
  ],
  "actor": "https://example.com/users/alice",
  "cc": [],
  "id": "https://example.com/objects/0185f5f8-10b5-1b69-f45e-25f06792f411",
  "object": "https://example.net/users/bob/posts/141892712081205472",
  "proof": {
    "created": "2023-01-28T01:22:40.183273595Z",
    "proofPurpose": "assertionMethod",
    "proofValue": "z5djAdMSrV...",
    "type": "MitraJcsRsaSignature2022",
    "verificationMethod": "https://example.com/users/alice#main-key"
  },
  "to": [
    "https://example.net/users/bob",
    "https://www.w3.org/ns/activitystreams#Public"
  ],
  "type":"Like"
}
```

### Supported proof suites

#### MitraJcsRsaSignature2022

Canonicalization algorithm: JCS  
Hashing algorithm: SHA-256  
Signature algorithm: RSASSA-PKCS1-v1_5

#### MitraJcsEip191Signature2022

Canonicalization algorithm: JCS  
Hashing algorithm: KECCAK-256 (EIP-191)  
Signature algorithm: ECDSA (EIP-191)

#### MitraJcsEd25519Signature2022

Canonicalization algorithm: JCS  
Hashing algorithm: BLAKE2b-512  
Signature algorithm: EdDSA

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

[FEP-c390](https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-c390.md) identity proofs are not supported yet.

## Account migrations

After registering an account its owner can upload the list of followers and start the migration process. The server then sends `Move` activity to each follower:

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "actor": "https://server2.com/users/alice",
  "id": "https://server2.com/activities/00000000-0000-0000-0000-000000000001",
  "object": "https://server1.com/users/alice",
  "target": "https://server2.com/users/alice",
  "to": [
    "https://example.com/users/bob"
  ],
  "type": "Move"
}
```

Where `object` is an ID of old account and `target` is an ID of new account. Actors identified by `object` and `target` properties must have at least one identity key in common to be considered aliases. Upon receipt of such activity, actors that follow `object` should un-follow it and follow `target` instead.

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
