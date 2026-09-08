# Authentication — unofficial API

Full narrative and every corroborating source: `docs/research/tidal-api.md` §3 (and §2 for hosts,
§14 for legal posture on client-id use). This file is the implementation-ready distillation.

## Table of contents

1. Device-code flow (RFC 8628)
2. PKCE / authorization-code flow
3. Scopes
4. Client IDs — which pair to use, and how to store the fallback
5. Token refresh
6. Session bootstrap (`GET /v1/sessions`) — and its one notable non-user, Strawberry
7. Logout / token revocation
8. Secure token storage per OS
9. The `client_unique_key` persistence rule (read this before you copy python-tidal)
10. reCAPTCHA and why device code is the safer default
11. Runtime client-id discovery as a revocation fallback

---

## 1. Device-code flow (RFC 8628)

```
POST https://auth.tidal.com/v1/oauth2/device_authorization
Content-Type: application/x-www-form-urlencoded

client_id=<ecosystem client id>&scope=r_usr%20w_usr%20w_sub
```
Sone additionally sends `client_secret` when it has one; python-tidal sends only `client_id`.
Response is **camelCase** (a real RFC 8628 gotcha): `deviceCode`, `userCode`, `verificationUri`,
`verificationUriComplete`, `expiresIn`, `interval`. `verificationUri` is `link.tidal.com`;
`verificationUriComplete` comes back **without a scheme** — print `https://` + that value.
(`ref:python-tidal/tidalapi/session.py:71-97` `LinkLogin`, `ref:sone/src-tauri/src/tidal_api.rs`
`start_device_auth`.)

Poll:
```
POST https://auth.tidal.com/v1/oauth2/token
client_id=<same id>
&client_secret=<if you have one>
&device_code=<deviceCode>
&grant_type=urn:ietf:params:oauth:grant-type:device_code
&scope=r_usr w_usr w_sub
```
Loop at `interval` seconds until `expiresIn` elapses or the body reports `error == "expired_token"`.
Sone treats HTTP 400 with `authorization_pending` or `slow_down` in the body as "keep waiting."
(`ref:python-tidal/tidalapi/session.py` `_check_link_login`, `ref:sone/src-tauri/src/tidal_api.rs`
`poll_device_token`.)

**Failure mode to handle explicitly**: a client id that is not registered as a Limited Input Device
returns an error body containing `not a Limited Input Device client` or `sub_status":1002`. Detect
this string and tell the user their client id is probably a web-player id, not a device-flow id
(`ref:sone/src-tauri/src/tidal_api.rs` ~line 1576).

Success response: `{ access_token, refresh_token, token_type: "Bearer", expires_in, user: {...} }`.

## 2. PKCE / authorization-code flow

```
https://login.tidal.com/authorize
  ?response_type=code
  &redirect_uri=https://tidal.com/android/login/auth
  &client_id=<PKCE client id>
  &lang=EN
  &appMode=android
  &client_unique_key=<hex of a random 64-bit int — persist this, see §9 below>
  &code_challenge=<base64url(sha256(verifier)), unpadded>
  &code_challenge_method=S256
  &restrict_signup=true
```
`code_verifier` = 32 random bytes, base64url-encoded, one trailing char stripped to drop the `=`
(`base64.urlsafe_b64encode(os.urandom(32))[:-1]` in python-tidal). Sone builds the **same parameter
set in the same order**, not a byte-identical URL — its `client_unique_key` is always 16 hex chars
(`format!("{:016x}", …)`) while python-tidal's is 1–16 chars (Python's `"02x"` format only pads to a
*minimum* of 2). Functionally equivalent; do not assume wire-identical.

The redirect target `https://tidal.com/android/login/auth` is a page that does not exist ("Oops"),
so the client must capture the URL. Four strategies in the wild:

- **Paste the URL** — python-tidal prompts for it; High Tide shows an entry row. No browser
  integration needed; works headless.
- **Embedded webview** watching for navigation to the redirect URI — Sone's approach (Tauri
  `WebviewWindow`).
- **Custom URI scheme** — Strawberry registers `tidal://login/auth`, no local server
  (`set_use_local_redirect_server(false)`).
- **Local HTTP server** — mopidy-tidal on port 8989; tidal-cli (official API) on
  `http://localhost:17893/callback`.

Token exchange:
```
POST https://auth.tidal.com/v1/oauth2/token
code=<code>
&client_id=<PKCE client id>
&grant_type=authorization_code
&redirect_uri=https://tidal.com/android/login/auth
&scope=r_usr+w_usr+w_sub          # note the literal + characters, not spaces
&code_verifier=<verifier>
&client_unique_key=<same key as authorize>
```
Both python-tidal and Sone send `scope` with literal `+` here while the device flow sends spaces.
Both codebases do this; the server accepts both. Not a bug to "fix."

## 3. Scopes

Unofficial API: three coarse scopes — `r_usr` (read user data), `w_usr` (write user data), `w_sub`
(write subscription). Strawberry requests only `r_usr w_usr`; tidalrs varies by call.

Official Developer API uses fine-grained scopes (tidal-cli's list, the fullest found in source):
`collection.read`, `collection.write`, `playlists.read`, `playlists.write`, `playback`, `user.read`,
`recommendations.read`, `entitlements.read`, `search.read`, `search.write`.

The official auth module also supports **three grant types beyond the ones above**, against the
same `auth.tidal.com/v1/oauth2/*` host: device code, PKCE, and `client_credentials` (app-only, no
user — the option for a metadata-only path such as a share-link resolver or a logged-out `search`
subcommand). `ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:218,334,451,480,525`.

## 4. Client IDs — which pair to use, and how to store the fallback

The PKCE pair is the only way to get `HI_RES_LOSSLESS` (python-tidal's own docstring). The
`urlpostpaywall` shortcut is **disabled** under a PKCE session in python-tidal
(`ref:python-tidal/tidalapi/media.py:410-417`, raises `URLNotAvailable`) — the PKCE identity gets
DASH manifests, not plain URLs. Sone skips the Hi-Res tiers entirely when it has no `client_secret`
configured, because those credentials typically return encrypted DASH streams that require
Widevine — see `references/playback.md` §Encryption.

How reference projects ship credentials:

| Project | Ships an ID? | User-replaceable? |
|---|---|---|
| python-tidal | yes, double-base64'd in `Config` | only by subclassing/patching |
| High Tide | inherits python-tidal's (pins `tidalapi-0.8.8`) | no |
| Sone | yes, XOR-masked, both pairs | **yes** — Settings; falls back to embedded |
| Strawberry | optional compile-time, encrypted | **yes** — "use custom client ID" checkbox |
| tidalt | yes, plaintext with a comment | no |
| tidalrs | no — caller passes one in | n/a (library) |
| tidal-cli | yes, its own **registered** official-API client `PYVtmSHMTGI9oBUs` | no |

**streamboat's posture: make the client id a first-class, user-replaceable setting, defaulting to
the ecosystem pair, exactly as Sone and Strawberry do.** This converts "TIDAL revoked the key, the
app is dead" into "paste a different key," and it is the single highest-leverage decision for
distro-packaging (a Debian ftpmaster objects far less to a user-supplied-credential mode).

Do not attribute a rationale-for-obfuscation comment to python-tidal — its source carries none (the
double-base64 split is genuinely just obfuscation, and the file's only nearby comments are
`# OAuth Client Authorization` / `# PKCE Client Authorization`). If you want an honest quote about
*why* a client ships this kind of credential, tidalt's is the one to use: "ClientSecret is the
public client secret baked into the official Tidal app; it is not a user credential"
(`ref:tidalt/internal/tidal/client.go:17-22`).

## 5. Token refresh

```
POST https://auth.tidal.com/v1/oauth2/token
grant_type=refresh_token
&refresh_token=<rt>
&client_id=<same id family that minted it>
&client_secret=<if you have one>
```
python-tidal switches between the PKCE and non-PKCE pair based on `session.is_pkce`. **The refresh
response may omit `refresh_token`** — fall back to the existing one (Sone's `RefreshResponse`
pattern); streamboat should too.

Refresh triggers, worst to best:

1. python-tidal — reactive, string-matched: refresh only when `userMessage` starts with `"The
   token has expired."` Fragile; do not copy.
2. Sone — refresh on any 401 **except** one carrying a 4xxx `subStatus` (those are playback errors,
   not auth errors — see `references/transport.md`). Copy this.
3. libopenTIDAL / tidalswift — refresh proactively, 300s / 5min before `expiresIn`. Best; combine
   with #2.

Guard concurrent refreshes with a single-flight lock — a page load fires a dozen parallel requests
and you want exactly one refresh, not a stampede. The official web SDK does this with a
`pending`/`pendingPromises` guard; tidalrs uses a `Semaphore` (its comment reads "Try to become the
single refresher" — do not attribute the phrase "to avoid thundering herd" to it, that quote does
not exist in its source).

The official SDK also has `grant_type=update_client` ("token upgrade"), for adding a client secret to
a previously public client. Not needed for the unofficial flow.

## 6. Session bootstrap: `GET /v1/sessions`

```
GET https://api.tidal.com/v1/sessions
Authorization: Bearer <access_token>
```
Returns `sessionId`, `countryCode`, `userId`. python-tidal, Sone, sone-windows and tidalt call this
immediately after obtaining a token. **Strawberry does not** — it takes `countryCode` straight from
the OAuth token response instead (`ref:strawberry/src/tidal/tidalservice.cpp:205-207`). Do not
assume every client calls this endpoint.

`countryCode` is mandatory on essentially every subsequent call. `sessionId` looks legacy — Sone
never sends it and works fine — but this is untested against endpoints python-tidal exercises that
Sone does not (`pages/*`, `genres`, `urlpostpaywall`). Treat "drop sessionId" as a reasonable default
with a compatibility flag to send it if some endpoint ever misbehaves without it.

Cheap login-validity check: `GET /v1/users/{userId}/subscription` — also tells you the tier.

## 7. Logout / token revocation

Not documented in any OSS client's README, but real and needed for an honest sign-out feature:

```
POST https://auth.tidal.com/v1/logout
Authorization: Bearer <access_token>
```
libopenTIDAL's man page: "This call requests a logout from TIDALs session layer. If successful the
tokens issued by the authorization server get invalidated. Further interaction with the API is not
possible without reauthenticating." (`ref:libopentidal/Source/OTService/OTServiceAuth.c:125-143`,
`Docs/OTServiceLogout.3`.) It is fire-and-forget — libopenTIDAL discards the response body.

**Do both**: call this endpoint, then erase local tokens regardless of the response. (The official
Android SDK's `logout()` only erases local tokens and never calls the server — that alone is not
enough for a real sign-out.)

## 8. Secure token storage per OS

| Project | Mechanism |
|---|---|
| High Tide | freedesktop Secret Service via libsecret; JSON blob with token-type/access/refresh/expiry/is-pkce. Explicitly unlocks the default collection when **not** under Flatpak. |
| Sone | settings JSON, encrypted at rest; master key in OS keyring with a file fallback. |
| Strawberry | QSettings with a custom obfuscation layer. |
| python-tidal / mopidy-tidal | plain JSON file; **drops `expiry_time`** — do not copy this, a reloaded session then has no expiry and relies entirely on reactive refresh. |
| tidalt | system keychain, age-encrypted file fallback. |
| tidal-sdk-android / ios | `EncryptedSharedPreferences` / Keychain. |

streamboat: OS keyring first (libsecret / Windows Credential Manager / macOS Keychain, e.g. via a
`keyring`-style crate), encrypted-file fallback for headless mode and Flatpak edge cases, file mode
0600. Persist `token_type`, `access_token`, `refresh_token`, `expiry_time`, `is_pkce`, `client_id`,
**and `client_unique_key`** (see next section) — do not repeat python-tidal's mistake of dropping
`expiry_time`. Inside a Flatpak sandbox you cannot unlock the keyring yourself — go through the
Secret portal; a headless/server mode has no keyring at all, so the encrypted-file fallback is not
optional, it is the primary mechanism there.

## 9. The `client_unique_key` persistence rule

**python-tidal generates a fresh `client_unique_key` in every `Config()` and never writes it to the
session file.** Copying this means streamboat registers a new "device" with TIDAL on every login.

The official SDK documents `clientUniqueKey` as "The unique key of the application," sends it on
both `authorize` and the token exchange, and **throws on refresh/token-upgrade if the
`clientUniqueKey` doesn't match the one that minted the token**
(`ref:tidal-sdk-web/packages/auth/src/auth/auth.ts:102,125,178-180,276-278,553`). TIDAL treats this
value as device identity, not a nonce.

**Generate this key once at first run, store it next to the refresh token, and send the identical
value on every authorize/exchange/refresh call.** Getting this wrong burns through TIDAL's
authorized-device cap and leaves stale phantom devices in the user's account settings.

## 10. reCAPTCHA and why device code is the safer default

The browser authorization-code flow (RFC 6749) is reCAPTCHA v3 protected; the device flow (RFC 8628)
is not. docTIDAL, the write-up behind libopenTIDAL: "I reversed engineered the TIDAL device
authorization grant (RFC 8628) since the web flow (RFC 6749) is reCaptcha v3 secured"
(`ref:tidal-fokka-engineering-/README.md`).

Consequences:
- Never attempt a scripted/headless POST of credentials to `login.tidal.com`.
- An embedded webview needs a realistic browser fingerprint or the challenge may score it as a bot —
  this is a plausible root cause for "login mysteriously fails in our webview" bugs.
- Device code is the reliable flow for headless/CLI and for constrained webviews for a reason beyond
  "no browser needed": it is simply not gated by the challenge at all.

## 11. Runtime client-id discovery as a revocation fallback

Beyond "make the client id user-replaceable," a second mitigation exists: the current web-player
client id can be read at runtime by visiting TIDAL's web player and pulling it from the login
redirect URL. `tidal-api-docs` records the current web id as `CzET4vdadNUFQ5JU` and advises "Ideally,
your implementation will dynamically retrieve the client_id from Tidal before making any requests"
(`ref:tidal-api-docs/Authorization/Retrieve-Current-Client-Id.md`, `README.md` — "Tidal is liable to
change their client_id at any time").

**Caveat**: a web-player id is not registered as a Limited Input Device, so it cannot do the
device-code flow (this is exactly the `sub_status":1002` failure in §1 above). Runtime discovery is
therefore only a fallback for the PKCE path. Recommendation: ship the ecosystem id as default, allow
a user override, and consider runtime discovery as an explicitly opt-in recovery mode — not the
default behavior.
