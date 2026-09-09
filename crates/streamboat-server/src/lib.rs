//! `streamboatd`'s library half: the HTTP + WebSocket control API (D-030,
//! D-031). Split out of the `streamboatd` binary so integration tests can
//! build the router directly against a [`streamboat_player::PlayerHandle`]
//! without going through a process boundary.

pub mod api;
