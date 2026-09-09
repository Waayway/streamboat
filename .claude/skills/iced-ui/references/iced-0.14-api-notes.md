# iced 0.14.0 API notes (verified against the pinned sources)

Verified by fetching `iced = "=0.14.0"` (features `tokio`, `image`, `svg`, `advanced`, `debug`) plus
`iced_test = "=0.14.0"` as a dev-dependency, then reading the extracted sources under
`~/.cargo/registry/src/*/`. Crate versions that resolved: `iced_core-0.14.0`, `iced_debug-0.14.0`,
`iced_futures-0.14.0`, `iced_graphics-0.14.0`, `iced_program-0.14.0`, `iced_renderer-0.14.0`,
`iced_runtime-0.14.0`, `iced_widget-0.14.2`, `iced_winit-0.14.0`, `iced_tiny_skia-0.14.1`,
`iced_wgpu-0.14.0`, `iced_test-0.14.0`. Line numbers are not cited since they drift across patch
releases of `iced_widget`; file names and signatures are what to grep for.

## 1. `application()` builder (`iced-0.14.0/src/application.rs`)

```rust
pub fn application<State, Message, Theme, Renderer>(
    boot: impl BootFn<State, Message>,
    update: impl UpdateFn<State, Message>,
    view: impl for<'a> ViewFn<'a, State, Message, Theme, Renderer>,
) -> Application<impl Program<...>>
```

- `BootFn<State, Message>` is implemented for any `Fn() -> C where C: IntoBoot<State, Message>`;
  `IntoBoot` is implemented for both `State` (no startup task) and `(State, Task<Message>)`.
- `UpdateFn<State, Message>` is implemented for `Fn(&mut State, Message) -> C where C: Into<Task<Message>>`
  — an update function can return `Task<Message>`, `()` (via a blanket `Into` for `Never`), or
  anything else with a `Task` conversion.
- `ViewFn<'a, State, Message, Theme, Renderer>` is implemented for `Fn(&'a State) -> Widget where
  Widget: Into<Element<'a, Message, Theme, Renderer>>` — returning a concrete widget type
  (`Column`, `Row`, ...) instead of `Element` directly is fine, `.into()` happens implicitly.
- Referencing a method as `Type::method` (not a closure) satisfies all three, plus `TitleFn`,
  `ThemeFn`, and the `subscription`/`style` builder parameters — see the idiom in §4.
- `Application<P>` builder methods after construction: `.settings(Settings)`, `.antialiasing(bool)`,
  `.default_font(Font)`, `.font(bytes)`, `.window(window::Settings)`, `.centered()`,
  `.exit_on_close_request(bool)`, `.window_size(Size)`, `.transparent(bool)`, `.resizable(bool)`,
  `.decorations(bool)`, `.position(window::Position)`, `.level(window::Level)`,
  `.title(impl TitleFn<State>)`, `.subscription(impl Fn(&State) -> Subscription<Message>)`,
  `.theme(impl ThemeFn<State, Theme>)`, `.style(impl Fn(&State, &Theme) -> theme::Style)`,
  `.scale_factor(impl Fn(&State) -> f32)`, ending in `.run() -> iced::Result`.
- `TitleFn<State>` accepts either `&'static str` or `Fn(&State) -> String`.
- `ThemeFn<State, Theme>` accepts `Theme` itself (a fixed theme) or `Fn(&State) -> T where T:
  Into<Option<Theme>>` — returning a bare `Theme` works because `Option<T>: From<T>` in `std`.

`iced::run(update, view)` (two-argument form, no title) is sugar for
`application(State::default, update, view).run()` and requires `State: Default`.

## 2. `Task<T>` (`iced_runtime-0.14.0/src/task.rs`)

Key constructors: `Task::none()`, `Task::done(value)`, `Task::perform(future, f: FnOnce(A) -> T)`,
`Task::run(stream, f: Fn(A) -> T)`, `Task::batch(tasks)`, `Task::stream(stream)`,
`Task::future(future)`. Combinators: `.map(f)`, `.then(f)`, `.chain(task)`, `.collect()`,
`.discard()`, `.abortable()`. `Task::perform`'s `future` must be `Send + 'static` when the default
(non-wasm) executor is used — an `async move` block capturing `Clone` handles (`ApiClient`,
`Arc<dyn PlayerLink>`, ...) satisfies this trivially.

`Task::perform`'s mapping closure is `FnOnce`, so it can move a value captured before the `.perform`
call (e.g. `let url_for_message = url.clone(); Task::perform(fetch(url), move |result| (url_for_message, result))`)
without fighting borrow checker — this is the shape `ui::images::fetch` uses.

## 3. `Subscription<T>` (`iced_futures-0.14.0/src/subscription.rs`)

```rust
Subscription::none() -> Self
Subscription::run<S>(builder: fn() -> S) -> Self               // S: Stream<Item = T> + MaybeSend + 'static
Subscription::run_with<D, S>(data: D, builder: fn(&D) -> S) -> Self  // D: Hash + 'static
Subscription::batch(subscriptions: impl IntoIterator<Item = Subscription<T>>) -> Self
Subscription::map<A>(self, f: Fn(T) -> A) -> Subscription<A>
```

`builder` in both `run` and `run_with` is a **bare `fn` pointer type**, not `impl Fn`. A closure with
no captured variables coerces to `fn` automatically and is the common case; a closure that captures
anything does not compile there — route captured data through `run_with`'s `D` parameter instead.
`D: Hash` is the only bound (no `Eq` needed) — iced hashes it to build a stable subscription
identity used to detect "is this the same subscription as last frame," so a `Hash` impl that is
merely *consistent*, not *unique*, is enough when there is structurally only ever one instance (see
`ui::player_link::LinkKey`, which hashes the `Arc<dyn PlayerLink>`'s pointer identity).

`iced::event::listen()` / `listen_with(f)` / `listen_raw(f)` (`iced_futures-0.14.0/src/event.rs`,
re-exported at `iced::event`) are the same shape: `f: fn(Event, event::Status, window::Id) ->
Option<Message>`, also a bare `fn` pointer. `event::Status::Captured` means some widget already
handled the event (e.g. a focused `text_input` consuming a keypress); check for
`Status::Ignored` before treating a keyboard shortcut as global, or a shortcut fires even while
typing in a text field.

## 4. The `Type::method` idiom, worked example

```rust
pub struct App { /* ... */ }

impl App {
    fn boot(ctx: Context, link: SharedLink) -> (Self, Task<Message>) { /* ... */ }
    fn update(&mut self, message: Message) -> Task<Message> { /* ... */ }
    fn view(&self) -> Element<'_, Message> { /* ... */ }
    fn title(&self) -> String { /* ... */ }
    fn theme(&self) -> Theme { /* ... */ }
    fn subscription(&self) -> Subscription<Message> { /* ... */ }
}

pub fn run() -> anyhow::Result<()> {
    let ctx = Context::load()?;
    let link: SharedLink = /* ... */;
    iced::application(move || App::boot(ctx.clone(), link.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .run()
        .map_err(Into::into)
}
```

`App::update` referenced bare has function-item type `fn(&mut App, Message) -> Task<Message>`,
exactly the shape the builder's `update` parameter wants — no closure wrapper needed. The `boot`
closure is the one exception: it is written as an explicit closure because it needs to bind `ctx`
and `link` from the enclosing scope. Since `application()`'s `boot` bound is `Fn() -> C` (not
`FnOnce`), the closure must not *move out* of `ctx` on each call — `ctx: Context` is
`#[derive(Clone)]` in streamboat specifically so `ctx.clone()` can be called repeatably inside a
`Fn` closure; a type that cannot cheaply derive `Clone` would need `RefCell<Option<T>>` +
`.borrow_mut().take()` instead (mutating through a shared reference, which still satisfies `Fn`).

## 5. Style closures (button/container/slider/checkbox/pick_list)

Every stock widget with visual state exposes `.style(impl Fn(&Theme, Status) -> Style + 'a)` (or,
for widgets with no interactive state like `container`, `.style(impl Fn(&Theme) -> Style + 'a)`).
`Status` enums are widget-specific:

- `button::Status`: `Active`, `Hovered`, `Pressed`, `Disabled`.
- `slider::Status`: has its own set (see `iced_widget::slider`); the default style function is
  `iced::widget::slider::default(theme, status) -> slider::Style`, useful as a base to tweak (e.g.
  `let mut style = slider::default(theme, status); style.rail.backgrounds = (muted.into(),
  muted.into()); style`) rather than reconstructing every field.
- `container::Style { text_color: Option<Color>, background: Option<Background>, border: Border,
  shadow: Shadow, snap: bool }`; `button::Style` adds nothing beyond swapping `text_color: Option<Color>`
  for a required `text_color: Color`.

Preset style functions exist per widget module for the five/six semantic tones: `button::primary`,
`::secondary`, `::success`, `::warning`, `::danger`, `::text`, `::background`, `::subtle` (all `fn(&Theme,
Status) -> Style`) — pass one directly as `.style(button::text)` when a preset is close enough,
which is simpler than writing a token-driven closure for something like a plain text-link button.

## 6. Widget constructor signatures worth double-checking before use

(`iced_widget-0.14.2/src/helpers.rs` unless noted)

- `text_input(placeholder: &str, value: &str) -> TextInput` — builder methods `.on_input(Fn(String)
  -> Message)`, `.on_submit(Message)`, `.secure(bool)` (password masking).
- `slider(range: RangeInclusive<T>, value: T, on_change: Fn(T) -> Message) -> Slider` — `T: Copy +
  From<u8> + PartialOrd`, **and** `T: Into<f64>` is additionally required for `Slider: Into<Element>`
  (checked in `iced_core`'s `Element: From<Slider<...>>` impl, not in `slider()`'s own signature, so
  the error surfaces at the `.into()`/`row![...]` call site instead of the `slider(...)` call
  itself). `u64` fails this; `u32`, `f32`, `f64` and the smaller integer types work.
- `checkbox(is_checked: bool) -> Checkbox` (`iced_widget/src/checkbox.rs`) — the label is
  `.label(impl text::IntoFragment)`, a separate builder call, not a constructor argument.
- `pick_list(options: L, selected: Option<V>, on_selected: Fn(T) -> Message) -> PickList` where `L:
  Borrow<[T]>`, `V: Borrow<T>`, `T: ToString + PartialEq + Clone`. A `[T; N]` array value does not
  satisfy `Borrow<[T]>` on its own (no blanket impl for fixed-size arrays); pass `.to_vec()` for an
  owned, lifetime-free `Vec<T>`, or an explicit `&'static [T]` slice reference for a `const` array
  (rvalue static promotion through a trivial `.as_slice()` call is not something to rely on without
  testing it — `.to_vec()` sidesteps the question entirely).
- `image(handle: impl Into<Handle>) -> Image` — `Handle::from_path`, `Handle::from_bytes` (encoded
  image bytes, format sniffed), `Handle::from_rgba(width, height, pixels: impl Into<Bytes>)` (already
  decoded RGBA8; `Vec<u8>: Into<Bytes>`).
- `svg(handle: impl Into<svg::Handle>) -> Svg` (feature `svg`), same shape as `image`.
- `scrollable(content) -> Scrollable`, `mouse_area(content) -> MouseArea`, `stack(children) ->
  Stack`, `responsive(f: Fn(Size) -> Element) -> Responsive` — all present and unchanged in spirit
  from earlier iced versions.
- **No flow/wrap widget.** `Row`, `Column`, `Grid` (fixed tracks) exist; a `Wrap` that reflows
  variable-count children onto new lines based on available width does not. Chunk items into
  fixed-size `row!`s inside a `column!` by hand for a "cards wrap to the next line" layout.
- `iced::widget::space::horizontal()` / `::vertical()` replace the old free functions
  `horizontal_space()`/`vertical_space()` (which are not re-exported at `iced::widget::` top level in
  0.14) — call through the `space` module.

## 7. `iced_test` — the headless simulator

`iced_test` is a **separate crate**, not an `iced` Cargo feature (`iced`'s own `tester` feature pulls
in `iced_tester`, an unrelated GUI test-recorder tool). Add it as:

```toml
[dev-dependencies]
iced_test = "=0.14.0"
```

Core API (`iced_test-0.14.0/src/simulator.rs`):

```rust
pub fn simulator<'a, Message, Theme, Renderer>(
    element: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Simulator<'a, Message, Theme, Renderer>
where
    Theme: theme::Base,
    Renderer: core::Renderer + core::renderer::Headless;

impl<'a, Message, Theme, Renderer> Simulator<...> {
    pub fn find<S: Selector>(&mut self, selector: S) -> Result<S::Output, Error>;
    pub fn click<S: Selector>(&mut self, selector: S) -> Result<S::Output, Error>;
    pub fn point_at(&mut self, position: impl Into<Point>);
    pub fn tap_key(&mut self, key: impl Into<keyboard::Key>) -> event::Status;
    pub fn typewrite(&mut self, text: &str) -> event::Status;
    pub fn simulate(&mut self, events: impl IntoIterator<Item = Event>) -> Vec<event::Status>;
    pub fn snapshot(&mut self, theme: &Theme) -> Result<Snapshot, Error>;
    pub fn into_messages(self) -> impl Iterator<Item = Message>;
}
```

- `&str` implements `Selector` by matching a widget containing that text — `ui.find("Suggested New
  Albums")` (or `ui.click(...)`) is the idiom for "does this render the text I expect" /
  "click the thing labelled X."
- The default `Renderer` type parameter is `iced::Renderer` = `iced_renderer::fallback::Renderer<wgpu,
  tiny-skia>`, whose `Headless::new` tries the wgpu backend first and falls back to `iced_tiny_skia`
  (pure CPU rasterisation) if wgpu initialization fails. **Confirmed empirically in this project's own
  sandboxed build container (no GPU, no display server, no X11/Wayland socket): the simulator still
  constructs and runs correctly** — it fell back to tiny-skia silently. A headless UI test needs the
  `iced_test` dev-dependency and nothing display-related; do not add `xvfb-run` or a virtual display
  to CI for these tests, it is not needed.
- Pattern for a screen test: build a synthetic `State` (or the screen's plain data struct) by hand,
  call `simulator(state.view(tokens))` (or `simulator(view(tokens, &state, ...))` for a screen with a
  free `view` function instead of a method), then `ui.find("some expected text").is_ok()` assertions.
  `simulator()`'s `element` parameter takes ownership of the `Element`, which itself may borrow from
  the `state`/`tokens` passed to `view` — keep those bindings alive for at least as long as `ui`.
- `ui.click(selector)` performs a real click sequence (point the cursor at the target's centre, then
  press+release) and returns the target; `ui.into_messages()` drains every `Message` the simulated
  interaction produced, for feeding into the screen's own `update` in the test to assert on resulting
  state — this project's screen tests stick to `find`-only assertions (render correctness) since none
  of them needed interaction-simulation to prove out, but the capability is there for
  a future test that needs it.

## 8. Multi-window: `iced::daemon` and the `window` module (`iced-0.14.0/src/daemon.rs`, `iced_runtime-0.14.0/src/window.rs`, `iced_core-0.14.0/src/window/*.rs`)

Verified for streamboat's D-036 mini-player (a second, always-on-top window over the same
`App` state) by reading the pinned sources directly, the same way §1-§8 were.

```rust
pub fn daemon<State, Message, Theme, Renderer>(
    boot: impl application::BootFn<State, Message>,
    update: impl application::UpdateFn<State, Message>,
    view: impl for<'a> daemon::ViewFn<'a, State, Message, Theme, Renderer>,
) -> Daemon<impl Program<...>>
```

- **`iced::daemon(boot, update, view)` reuses `application::BootFn`/`UpdateFn` verbatim** (same
  `boot() -> (State, Task<Message>)` shape as `application()`, §1) but its own `ViewFn`/`TitleFn`/
  `ThemeFn` traits all carry an extra `window::Id` parameter: `view(&'a State, window::Id) ->
  Widget`, `title(&State, window::Id) -> String`, `theme(&State, window::Id) -> impl Into<Option<Theme>>`.
  Referencing `App::view`/`App::title`/`App::theme` as bare methods with matching signatures
  satisfies these the same `Type::method` way §4 describes — just with one more parameter now.
  `subscription`/`style` are unchanged (`Fn(&State) -> Subscription<Message>` /
  `Fn(&State, &Theme) -> theme::Style`, no `window::Id`): one subscription for the whole daemon
  across every window, not per-window.
- **A `Daemon` opens no window on its own and never exits when its last window closes** — both
  documented directly on `daemon()` itself (`Program::window()` returns `None` for a `Daemon`,
  unconditionally, vs. `application()`'s builder-supplied `Settings`). This is what makes D-014's
  "closing the window keeps playing app alive, quitting is explicit" trivial: the app simply never
  auto-exits, no workaround needed. Verified by reading `iced_winit-0.14.0/src/lib.rs`'s handling of
  `WindowEvent::Destroyed`: the "exit when `window_manager.is_empty()`" branch is itself gated
  `if !is_daemon`, so a `Daemon`'s windows can all close without the process exiting — `application()`
  (a non-daemon `Program`) does not get this for free.
- **`window::open(settings: window::Settings) -> (window::Id, Task<window::Id>)`** returns the new
  `Id` *synchronously* — `Id::unique()` is called immediately, before the window actually exists —
  and separately a `Task` that performs the real open. Store the `Id` right away (e.g. in `App`
  returned from `boot`); don't wait on the `Task` to know it. The `Task<window::Id>` needs
  `.discard::<Message>()` (§2: `Task<T>::discard<O>(self) -> Task<O>`, generic over the discarded
  type) to fold into a `Task<Message>` batch, since its own output type is `window::Id`, not
  `Message`.
- **`window::close::<T>(id) -> Task<T>`** closes one window by id — generic over the task's output
  type, so `T` is inferred as `Message` at the call site with no explicit turbofish needed in
  practice.
- **Closing a window is intercepted through `exit_on_close_request`, not by racing the subscription.**
  `window::Settings::exit_on_close_request` defaults to `true`; when the OS delivers a native
  close-button press with that flag true, `iced_winit`'s shell runs `window::Action::Close` *itself*,
  destroying the window, in addition to (not instead of) delivering the ordinary
  `window::Event::CloseRequested` to the app. Verified in `iced_winit-0.14.0/src/conversion.rs`:
  `conversion::window_event` maps `winit::event::WindowEvent::CloseRequested` to
  `Some(Event::Window(window::Event::CloseRequested))` **unconditionally**, regardless of
  `exit_on_close_request` — that special-cased auto-close is a separate, additional action layered on
  top, not a gate on whether the app-visible event fires. So to actually *prevent* the close (D-014:
  hide instead), `exit_on_close_request: false` is required on that window's `Settings`; only then
  does `window::close_requests() -> Subscription<window::Id>` become the *only* thing that happens,
  leaving `update` free to answer it with `window::set_mode(id, window::Mode::Hidden)` instead of
  `window::close`.
- **Hide/show a window through `window::Mode`, there is no dedicated `hide`/`show` action.**
  `window::Mode` is `Windowed | Fullscreen | Hidden`; `window::set_mode::<T>(id, mode) -> Task<T>`
  and its counterpart `window::mode(id) -> Task<Mode>` are the only way to toggle visibility from
  application code (`iced_winit`'s handling of `Action::SetMode` maps `Mode` to
  `window.raw.set_visible(...)` internally). `window::gain_focus::<T>(id) -> Task<T>` brings a
  *visible* window to the front — its own doc comment says it "has no effect if ... not visible," so
  un-hiding then focusing is `window::set_mode(id, Mode::Windowed).chain(window::gain_focus(id))`
  (`Task::chain`, §2, runs the two in sequence — `Task::batch` would not guarantee the un-hide lands
  first).
- **`window::Level::AlwaysOnTop`** (alongside `Normal`, `AlwaysOnBottom`) on `window::Settings::level`
  is the always-on-top flag a floating utility window (streamboat's mini-player) wants; set once at
  `window::open` time via `Settings`, no separate action needed to turn it on afterward unless it
  needs to change later (`window::set_level::<T>(id, level) -> Task<T>` exists for that).
- **`window::close_requests() -> Subscription<window::Id>`**, **`close_events() -> Subscription<window::Id>`**,
  **`open_events() -> Subscription<window::Id>`**, **`events() -> Subscription<(window::Id, window::Event)>`**
  are the four ready-made subscriptions over `iced::event::listen_with` — each already filters to one
  `window::Event` variant (or all of them, for `events()`) and unwraps the `window::Id` alongside it,
  cheaper than writing the `listen_with` match arm by hand for these four common cases.

## 9. Things that look like they should exist but don't (as of 0.14.2)

- No module-level `text_input::focus(id)`/`Task`-returning focus helper was found in
  `iced_widget::text_input` — only an inherent `TextInput::focus(&mut self)` on the widget's internal
  runtime state, not something a `Message` handler can trigger directly without deeper
  `widget::operation` plumbing. streamboat's ctrl+f shortcut navigates to the Search screen but does
  not additionally force text-input focus; revisit if `iced_widget::operation::focusable` grows a
  simpler entry point in a later 0.14.x patch.
- No `horizontal_space()`/`vertical_space()` free functions at `iced::widget::` top level (see §6) —
  use `iced::widget::space::horizontal()`/`::vertical()`.
- No `Wrap`/flow layout widget (see §6).
- No dedicated modal/dialog/popup widget — see §9 for the `stack!` + dimmed-`container` shape that
  substitutes for one.

## 9. Modals (`stack!`), programmatic scroll (`operation::scroll_to`), and eager-vs-lazy futures

Verified building the "add to playlist" picker (`ui::actions`) and the Lyrics screen's
auto-scroll.

**`stack!`** (`iced_widget-0.14.2/src/lib.rs`'s macro, or the `stack(children)` free function in
`helpers.rs`) layers children back-to-front; each child is converted with `Element::from`, so an
already-built `Element` or anything `Into<Element>` (a bare `Container`, `Column`, ...) drops in
directly:

```rust
stack![
    base_ui,                         // Element<'_, Message> — whatever screen opened the modal
    container(modal_content)         // Container<'_, Message> — Into<Element> works too
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Color { a: 0.55, ..Color::BLACK }.into()),
            ..container::Style::default()
        }),
]
```

`Container::center_x(width: impl Into<Length>)`/`center_y(height: impl Into<Length>)`
(`iced_widget-0.14.2/src/container.rs`) set the length *and* centre the content on that axis in one
call — simpler than `.width(Length::Fill).align_x(...)`, whose `align_x`/`align_y` additionally need
an `impl Into<alignment::Horizontal>`/`Into<alignment::Vertical>` rather than a bare
`iced::Alignment`.

**`iced::widget::operation::{scroll_to, snap_to, scroll_by, AbsoluteOffset, RelativeOffset}`** — a
`Task<T>`-returning family for driving a `scrollable` from `update` rather than `view`. They live in
`iced_runtime::widget::operation` (`iced_runtime-0.14.0/src/widget/operation.rs`), re-exported at
`iced::widget::operation` through `iced`'s `pub use iced_runtime::widget::*;` — *not* re-exported
under `iced::widget::scrollable`, which only re-exports the two offset types
(`iced_widget-0.14.2/src/scrollable.rs`: `pub use operation::scrollable::{AbsoluteOffset,
RelativeOffset};`). Give the target `scrollable` a stable id first:

```rust
scrollable(content).id(iced::widget::Id::new("lyrics-lines"))
```

then, from `update`, whenever the thing that should be visible changes:

```rust
iced::widget::operation::scroll_to(
    iced::widget::Id::new("lyrics-lines"),   // same id, re-constructed — Id is just a wrapped string/u64
    iced::widget::operation::AbsoluteOffset { x: None, y: Some(offset_px) },
)
```

`AbsoluteOffset<T = f32>` (`iced_core-0.14.0/src/widget/operation/scrollable.rs`) is generic with a
default type param, not two unrelated types of the same name — but its two call shapes use
different instantiations, and mixing them up is the actual pitfall. *Setting* a position always
wants `AbsoluteOffset<Option<f32>>` — both the public `operation::scroll_to` above and the
`operation::Scrollable for State` trait method it dispatches to internally
(`fn scroll_to(&mut self, offset: AbsoluteOffset<Option<f32>>)`) take exactly this shape, an axis
left `None` staying untouched. *Reading* the current position uses the bare default instead —
`Viewport::absolute_offset(&self) -> AbsoluteOffset` (i.e. `AbsoluteOffset<f32>`, no operation
involved, just a getter). Nothing in `iced_test`'s `Simulator` can confirm a scroll operation
actually moved the viewport — verify visually, or restrict testing to the pure "what index should
be visible" logic that decides *when* to issue the scroll.

**A future passed to `Task::perform` is only lazy from its first `.await` onward.** Anything
evaluated in the expression *before* that — including the call that produces the future value
itself, if that call does work at invocation time rather than at first poll — runs immediately,
synchronously, wherever `Task::perform` is called. `tokio::time::sleep(duration)` is exactly such a
call: it calls `tokio::runtime::Handle::current()` to register the timer *when invoked*, not when
first polled, so

```rust
Task::perform(tokio::time::sleep(duration), move |()| Message::Expire(id))   // panics with no reactor
```

panics with `there is no reactor running, must be called from the context of a Tokio 1.x runtime`
the instant this line executes anywhere without a live tokio context — a plain `#[test]` most
concretely, since it has no runtime at all, but the same eager evaluation would just as readily blow
up if this code path were ever reached before the app's own runtime is attached. The fix is an
`async move` block, which defers everything inside it — the `tokio::time::sleep(...)` call included
— to actual poll time:

```rust
Task::perform(async move { tokio::time::sleep(duration).await }, move |()| Message::Expire(id))
```

`ui::screens::search`'s debounce helper already did this correctly (an `async fn` body is lazy in
its entirety, since calling an `async fn` just builds a state machine); `ui::banner`'s auto-expire
needed the same wrapping once written as an inline expression instead of a named `async fn`.
