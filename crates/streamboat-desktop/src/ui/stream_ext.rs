//! One tiny type alias shared by [`crate::ui::player_link`] and
//! [`crate::ui::images`]: a boxed, `Send`, pinned stream, exactly the shape
//! `iced::Subscription::run_with` needs.

use std::pin::Pin;

use futures::Stream;

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;
