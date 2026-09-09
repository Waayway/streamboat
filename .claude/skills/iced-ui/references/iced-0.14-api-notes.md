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
  of the wave-1 screens needed interaction-simulation to prove out, but the capability is there for
  a future test that needs it.

## 8. Things that look like they should exist but don't (as of 0.14.2)

- No module-level `text_input::focus(id)`/`Task`-returning focus helper was found in
  `iced_widget::text_input` — only an inherent `TextInput::focus(&mut self)` on the widget's internal
  runtime state, not something a `Message` handler can trigger directly without deeper
  `widget::operation` plumbing. streamboat's ctrl+f shortcut navigates to the Search screen but does
  not additionally force text-input focus; revisit if `iced_widget::operation::focusable` grows a
  simpler entry point in a later 0.14.x patch.
- No `horizontal_space()`/`vertical_space()` free functions at `iced::widget::` top level (see §6) —
  use `iced::widget::space::horizontal()`/`::vertical()`.
- No `Wrap`/flow layout widget (see §6).
