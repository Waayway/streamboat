---
name: iced-ui
description: Pinned-version facts and patterns for streamboat's iced 0.14.0 desktop shell (D-013) — the application()/Task/Subscription builder API as it actually is in 0.14 (not 0.9-0.12 tutorials), the button/container/slider Style-closure pattern the design-token theme relies on, and the iced_test headless simulator used for widget-tree assertions without a display. Use whenever writing or reviewing code under crates/streamboat-desktop/src/ui/, adding an iced dependency or feature, or writing a headless UI test — D-013 asks explicitly to keep the pinned iced docs in the repo's agent context so agent-written iced code gets reviewed against the real 0.14 API instead of remembered pre-0.13 patterns that no longer compile.
---

# iced 0.14 for streamboat

Source of truth: read the pinned crate sources directly before trusting this skill or general
knowledge of iced — `iced`'s API changed hard across 0.9 to 0.14 (D-013), so anything that sounds
like a half-remembered tutorial is exactly the failure mode this skill exists to prevent. The
sources live in the cargo registry once fetched: `~/.cargo/registry/src/*/iced-0.14.0`,
`iced_widget-0.14.2`, `iced_core-0.14.0`, `iced_runtime-0.14.0`, `iced_futures-0.14.0`,
`iced_program-0.14.0`, `iced_test-0.14.0`. If they are not present, `cargo fetch` in a crate that
depends on `iced = "=0.14.0"` pulls them down — do this before writing iced code from memory.

`references/iced-0.14-api-notes.md` is the detailed reference (full signatures, worked examples,
the simulator API); this file is the index and the pitfalls.

## The version pin (D-013)

`iced = "=0.14.0"` **exactly** — a caret/tilde range is wrong here. iced's API breaks between minor
versions in ways that are silent at the type level in some places (a renamed helper, a changed
trait bound) and loud in others; pinning is what makes "review agent-written iced code against the
pinned API" (D-013) a well-defined activity instead of a moving target. `iced_test` is pinned
separately as `iced_test = "=0.14.0"` (see below for why it's a separate crate at all).

## Pitfalls (the ones that cost time building this shell)

1. **`iced::run`/`iced::application` dropped the title argument.** Pre-0.13 tutorials show
   `iced::run("My App", update, view)`; in 0.14 it's `iced::run(update, view)` (two args, `State:
   Default`) or, for a real app, `iced::application(boot, update, view).title(title_fn)...run()`.
   Passing a string literal as the first argument to `iced::run` fails to compile with a confusing
   trait-bound error about `UpdateFn`, not an obvious "wrong argument" message.
2. **`application()`'s three functions are `boot`, `update`, `view` — not `new`/`update`/`view`.**
   `boot: impl Fn() -> C where C: IntoBoot<State, Message>` (a plain `State` value, or
   `(State, Task<Message>)`, both implement `IntoBoot`) is called to produce the initial state and
   an optional startup `Task`. Builder methods chain after: `.title(...)`, `.theme(...)`,
   `.subscription(...)`, `.style(...)`, `.window(...)`, `.centered()`, `.scale_factor(...)`, ending
   in `.run() -> iced::Result`.
3. **`boot`/`title`/`theme`/`subscription`/`style` are `Fn`, not `FnOnce` or `FnMut`.** A closure
   passed to `.title(|state| ...)` etc. must not *move out* of its captures, since iced may call it
   more than once conceptually (and definitely expects repeatable, side-effect-free calls). If you
   need to hand ownership of something non-`Clone` into `boot` exactly once, wrap it in
   `RefCell<Option<T>>` and `.take()` inside the `Fn` closure (interior mutability satisfies `Fn`
   even though the call mutates) — or, simpler, make the type `Clone` if all its fields already are
   (streamboat's own `Context` is `#[derive(Clone)]` for exactly this reason: every field it holds —
   `ApiClient`, `Arc<EncryptedFileStore>`, `AppDirs`, `Settings`, `DeviceIdentity` — was already
   `Clone`, so deriving it on the struct cost nothing and let `boot` do `ctx.clone()` per call).
4. **Methods referenced as `Type::method` line up with the builder's `Fn(&State, ...) -> T`
   parameters automatically** — `App::update` (defined as `fn update(&mut self, message: Message) ->
   Task<Message>`) has function-item type `fn(&mut App, Message) -> Task<Message>`, which is
   exactly `UpdateFn<App, Message>`'s shape; the same trick works for `view`/`title`/`theme`/
   `subscription`. No closures needed at the call site: `iced::application(boot, App::update,
   App::view).title(App::title).theme(App::theme).subscription(App::subscription).run()`.
5. **`Subscription::run`/`run_with` and `iced::event::listen_with` take a bare `fn` pointer, not a
   general closure.** `Subscription::run(builder: fn() -> S)`, `Subscription::run_with(data: D, builder:
   fn(&D) -> S) where D: Hash`, `listen_with(f: fn(Event, event::Status, window::Id) -> Option<Message>)`
   — a closure with no captured variables coerces to a `fn` pointer automatically and works fine; a
   closure that captures anything does not compile there. To thread per-app data (e.g. "which
   `PlayerLink` to subscribe to") into `run_with`, pass it as the `D` parameter, not as a capture —
   `D` just needs `Hash + 'static`. streamboat's own `ui::player_link::LinkKey` wraps
   `Arc<dyn PlayerLink>` and hashes the `Arc`'s pointer identity, since the app only ever holds one
   link for its whole lifetime and Arc pointer identity is trivially a stable, unique subscription
   id — cheaper than inventing an id scheme on the trait itself.
6. **`slider`/`vertical_slider`'s value type `T` must implement `Copy + From<u8> + PartialOrd`, and
   `Element: From<Slider<'_, T, ...>>` additionally needs `T: Into<f64>`.** `u64` does *not*
   implement `Into<f64>` in `std` (a `u64` can lose precision as `f64`, so there's no blanket impl) —
   using `u64` milliseconds as a slider's value type fails with a `From<u64> for f64` error that
   only appears where the widget gets converted `.into()` an `Element`, several lines away from the
   actual `slider(...)` call. Cast track-position milliseconds to `u32` for the seek slider
   (durations never approach `u32::MAX` ms, about 49 days) rather than reaching for `f64` and losing
   integer semantics.
7. **`iced_widget` has no flow/wrap layout widget.** There is `Row`, `Column`, `Grid` (fixed
   column/row tracks, not a masonry/flow layout), `Wrap` does not exist as of 0.14.2. A "cards that
   wrap to the next line" grid (Home/Explore's card sections) has to be hand-chunked into fixed-size
   `row!`s stacked in a `column!` (see `ui::widgets::feed_sections`/`card_grid`) rather than reached
   for as a stock widget.
8. **`iced_test` is its own crate, not an `iced` feature.** `iced`'s `tester` feature pulls in
   `iced_tester`, a *different* thing — a GUI test-recorder/editor/runner tool, not what streamboat
   needs. The headless simulator (`Simulator`/`simulator()`, click/tap_key/typewrite/snapshot) lives
   in `iced_test`, added as its own `[dev-dependencies]` entry (`iced_test = "=0.14.0"`), independent
   of any feature flag on the main `iced` crate.
9. **`Simulator::new`/`simulator()` need `Renderer: core::renderer::Headless`, and the default
   `Renderer` type param resolves through `iced_renderer::fallback::Renderer<wgpu, tiny-skia>`,
   which tries wgpu first and falls back to tiny-skia (pure CPU rasterisation) if wgpu's `Headless::new`
   fails.** In a container with no GPU/display, wgpu headless init fails and the fallback to
   tiny-skia succeeds silently — the simulator works fine with zero display server, confirmed by
   running it in this repo's own sandboxed agent container. Nothing about a headless UI test needs
   `xvfb` or any display; it needs the `iced_test` dev-dependency and nothing else runtime-side.
10. **`checkbox`/`pick_list` constructor argument order is easy to get backwards.**
    `checkbox(is_checked: bool) -> Checkbox` takes *only* the boolean; the label is a separate
    builder method, `.label(text)`. `pick_list(options: L, selected: Option<V>, on_selected: impl Fn(T)
    -> Message)` requires `L: Borrow<[T]>` — passing a `[T; N]` array value directly does not satisfy
    that bound (arrays don't implement `Borrow<[T]>`, only slices and `Vec<T>` do); pass `.to_vec()`
    (simplest, no lifetime questions) or an explicit `&'static [T]` slice reference when the source is
    a `const` array.
11. **A `Message`/`Program::State` type does not need `Debug`/`Clone` in general, but `iced`'s
    `debug` feature makes `Message: Debug` a hard requirement of `Application::run`.** The bound is
    spelled `Message: MaybeDebug`, and `MaybeDebug` is `#[cfg(feature = "debug")] trait MaybeDebug:
    Debug {}` vs. `#[cfg(not(feature = "debug"))] trait MaybeDebug {}` (unconditionally implemented) —
    so turning on `debug` (worth it for devtools/hot-reload, D-013) silently turns "does my Message
    enum derive Debug" from optional into mandatory, transitively through every nested per-screen
    message type. A field that can't derive `Debug` for a good reason (streamboat's `PkceSession`
    holds a `Zeroizing<String>` verifier it must never print) needs a hand-written `Debug` impl that
    redacts that field, not a dropped derive.
12. **Text content into `text(...)`/`.label(...)` accepts both `&'a str` and owned `String` via
    `IntoFragment<'a>` for any `'a`** (`impl<'a> IntoFragment<'a> for String` — the owned case has no
    borrow to outlive anything) — pass owned `String`s built inside a `view` function directly rather
    than reaching for `Box::leak` or restructuring lifetimes to keep a `&str` alive; the leak is not
    just ugly, it is an actual per-render memory leak.

## Owner-context notes

- Theming (D-012, D-013): build a small `Tokens` struct (colours, radii, spacing scale, type scale)
  with `dark()`/`light()` constructors, feed it into `iced::Theme::custom(name, Palette { ... })` for
  the stock-widget base look, and read `Tokens` directly (captured `Copy` into style closures) for
  every custom container/card/badge style instead of trying to smuggle extra fields through
  `iced::Theme` itself. `iced::Theme`'s built-in `Palette` has exactly five fields — `background`,
  `text`, `primary`, `success`, `warning`, `danger` — there is no seventh slot for e.g. an "elevated
  surface" colour, which is exactly why the token struct, not the theme object, is the source of
  truth D-013 asks for.
- The engine seam (D-010) is `PlayerHandle::send`/`subscribe` and `Player::spawn` only — the iced
  shell never touches `streamboat_player::Engine` or a backend type directly. `Player::spawn`
  documents itself as needing "the current tokio runtime"; since iced's own `tokio` feature runs its
  `Task`/`Subscription` futures on iced's *own* internal tokio runtime, spawn the `Player` on a
  second, explicitly-built `tokio::runtime::Runtime` kept alive for the process's lifetime (a local
  variable in `ui::app::run` that outlives the blocking `.run()` call) — the two runtimes coexist
  fine since nothing here needs "the current runtime" for anything beyond the initial `Player::spawn`
  call itself.
- Keyboard shortcuts (task item 4) go through `iced::event::listen_with`, matched on
  `iced::keyboard::Event::KeyPressed` — space, ctrl+f and escape are exactly the three this wave
  wires up; media keys are explicitly MPRIS's job later, not this subscription's.
