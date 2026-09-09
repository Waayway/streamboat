//! Login flows (D-024): the device-code flow is the headless/CLI default;
//! PKCE is the desktop default and the only flow served HI_RES_LOSSLESS in
//! the clear.

pub mod device_code;
pub mod pkce;

/// Requested once and never changed: TIDAL's SDK logs users out when the
/// scope set of a refresh differs from the minted token's (`tidal-api` auth §12).
/// The device flow sends it with spaces.
pub const SCOPES: &str = "r_usr w_usr w_sub";
/// The PKCE exchange sends the same scopes joined by literal `+` characters,
/// as python-tidal and Sone do; the server accepts both spellings.
pub const PKCE_SCOPES: &str = "r_usr+w_usr+w_sub";
