# Mobile path, concretely

Mobile is future scope — must not be precluded, nothing mobile-specific ships now. This file ranks
the paths by how much of the desktop investment survives, for whenever the owner decides mobile is
near-term (see Open decision #4 in `SKILL.md`).

## §1 Rust core + Tauri mobile

`streamboat-core` and `streamboat-proto` compile for `aarch64-linux-android` and
`aarch64-apple-ios` unchanged, provided the app crate has the library shape in
`references/tauri-engineering-facts.md` §9. The React UI is reused with a responsive layout. What
must be rewritten: audio output (Tauri has no first-party audio plugin — you fork or write one
wrapping Media3 ExoPlayer on Android and AVPlayer/AVAudioEngine on iOS), background playback
service, lock-screen controls, offline cache policy. Bit-perfect is not a mobile goal anyway (phones
resample).

The one community option: `tauri-plugin-native-audio` 1.0.5 (2026-03-02, MIT/Apache-2.0) wraps
Android Media3 ExoPlayer + `MediaSessionService` + notification controls, and iOS AVPlayer +
`MPNowPlayingInfoCenter` + `MPRemoteCommandCenter`; requires Android 8.0+/iOS 14.0+ and the iOS
Background Modes → Audio capability. ~1,544 total downloads — essentially unproven. Treat it as a
starting point to fork, not a dependency to trust.

## §2 Rust core + UniFFI + native UIs

`streamboat-core` becomes an `.aar` and `.xcframework`; Android gets Compose, iOS gets SwiftUI, each
with its own ExoPlayer/AVPlayer. Most work, best result — the pattern the official TIDAL SDKs use
(`tidal-sdk-android`'s player module is separate from `streaming-api`, which is separate from `auth`;
`tidal-sdk-ios` likewise — mirror that module boundary, not a monolith). See
`references/architecture-shapes.md` §2 for the FFI-boundary cost.

## §3 Rust core + Flutter

One UI codebase for desktop and mobile; audio delegated to a Flutter audio plugin (`simple_audio`,
built on `flutter_rust_bridge`, or `media_kit`/libmpv) on mobile and to the Rust engine on desktop.
See `references/scoring-and-candidates.md` §2 (4.8) for the full tradeoff.

## The invariant that protects all three paths

**`streamboat-core` must not depend on any UI toolkit, any windowing library, or any desktop-only OS
API.** Enforce it with a CI job that builds `streamboat-core` for `wasm32-unknown-unknown` or a bare
`--no-default-features` profile — a compile-time check, not a design-review reminder.

## Comparanda this research did not reach

The brief named Symphonium and Flutter music apps generally (Harmony et al.) as comparanda; no
primary source was checked for either in this pass. The weakest evidence gap in this whole mobile
discussion: the report cites production Tauri **desktop** apps (Spacedrive, AppFlowy, Clash Verge)
but no example actually shipping on Android/iOS — the "Tauri mobile is usable" claim rests on the
framework's own release notes and the community plugin above, not a shipped precedent.
