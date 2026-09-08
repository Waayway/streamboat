# Official v2 OpenAPI catalogue

Source: `ref:tidal-sdk-web/packages/api/bin/tidal-api-oas.json` (identical copy at
`ref:tidal-sdk-android/tidalapi/bin/tidal-api.json` and
`ref:tidal-sdk-android/tidalapi/bin/openapi_downloads/tidal-api-oas.json`). **TIDAL API 1.10.104**,
server `https://openapi.tidal.com/v2`, 256 paths, 993 schemas. First-party and machine-readable —
not a web summary — and under-used in the first research pass on this codebase. Read this file
directly for anything you can't find summarized elsewhere in this skill.

Table of contents:
1. API conventions from the spec's own description
2. Paths already covered elsewhere in this skill (pointer table)
3. Feature-bearing paths not otherwise covered in this skill

## 1. API conventions from the spec's own description

The spec's `info.description` documents client-facing conventions worth building against from day
one if streamboat ever talks to `openapi.tidal.com/v2` directly:

- **Pagination**: cursor-only — `links.next` carries an opaque `page[cursor]`, no numeric offsets;
  `meta.total` is approximate.
- **Compound documents**: `include=` with a 3-level depth / 10-resource cap.
- **Filtering/sorting**: `filter[member]` and `sort=`/`-sort`.
- **Mutations**: an `Idempotency-Key` header is accepted on every mutation — replay-safe within 1
  hour, `409` while a request is in flight, `422` on a payload mismatch on retry. **Use this on
  every write streamboat performs.**
- **PATCH semantics**: three-state nullable fields (present-and-set, present-and-null,
  absent-means-unchanged) — do not assume a two-state PATCH.
- **Compression**: gzip over 2KB.
- **Deprecation**: a six-month guarantee before a field/endpoint is removed.
- **Forward compatibility, explicit in the spec**: "new values can be added to any enum at any
  time; treat unknown values as forward-compatible, not as errors." This is the spec's own version
  of the module-renderer/unknown-shape advice elsewhere in this skill — apply it to every enum you
  read from this API, not just page modules.

## 2. Paths already covered elsewhere in this skill

| Topic | Paths | See |
| --- | --- | --- |
| Collaborative playlists | `/collaborationInvites`, `/collaborationInviteRedemptions`, `playlists/{id}/relationships/{collaborators,collaboratorProfiles}` | `library-playlists-collections.md` §1 |
| Playlist visibility | `Playlists_Attributes.accessType` | `library-playlists-collections.md` §1 |
| Save for Later / folders | `/userCollectionSaveForLaters/{id}`, `/userCollectionFolders` | `library-playlists-collections.md` §2 |
| Cloud queue | `/playQueues`, `/playQueues/{id}` | `remote-playback-connect-controls.md` §1 |
| Offline / downloads | `/offlineTasks`, `/downloads`, `/installations`, `/installations/{id}/relationships/offlineInventory` | `entitlements-tiers-history.md` §3 |
| Search history/suggestions | `/searchResults`, `/searchSuggestions`, `/searchHistoryEntries/{id}` | `browse-pages-screens.md` §5 |
| Social/creator layer | `/comments`, `/reactions`, `/appreciations`, `/artistClaims`, `/purchases`, etc. | `social-feed-creator.md` §6 |
| Personalised mix families | `/userDailyMixes`, `/userDiscoveryMixes`, `/userNewReleaseMixes`, `/userOfflineMixes`, `/userRecommendations` | `browse-pages-screens.md` §4 |
| Media replacement / region unavailability | `.../relationships/replacement`, `replaceMedia=`, `/usageRules` | `quality-playback-queue.md` §3 |
| Cross-DSP sharing | `/dspSharingLinks`, `/shares`, `/savedShares` | `remote-playback-connect-controls.md` §7 |

## 3. Feature-bearing paths not otherwise covered in this skill

These appear in the spec but have no OSS-client implementation and are not detailed elsewhere in
this skill set. Listed so a future implementer knows they exist and can read the spec directly
rather than assuming this is the complete inventory:

`/dynamicPages`, `/dynamicModules` (a v2 generalisation of the page-module system —
cross-reference against `browse-pages-screens.md` §2 if you build the v2 renderer),
`/trackFiles/{id}`, `/trackManifests/{id}`, `/videoManifests/{id}` (manifest resources — see
`quality-playback-queue.md` §3 for the query-param usage already covered),
`GET /tracks?filter[isrc]=<isrc>` (ISRC → TIDAL track-id lookup, cursor-paginated like every other
v2 list — useful for matching a locally-known or other-service track to a TIDAL id; TidaLuna's own
client calls exactly this, `ref:TidaLuna/plugins/lib/src/classes/TidalApi/index.ts:96-97`),
`/userDataExportRequests` (GDPR-style data export — relevant if streamboat ever handles account
data on the user's behalf), `/terms`, `/acceptedTerms`, `/scopes`, `/temporaryUserTokens` (auth/
consent bookkeeping — cross-reference `docs/research/tidal-api.md` §3 for the auth flows this
skill doesn't re-derive).

If you need something not listed anywhere in this skill, the fastest path is: open the spec file,
search for the resource name, and read its `Attributes` schema and relationships directly — it is
993 schemas, more complete than any prose summary can stay in sync with.
