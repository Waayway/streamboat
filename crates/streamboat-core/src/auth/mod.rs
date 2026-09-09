//! Login flows. This spike ships the device-code flow (RFC 8628, the
//! headless/CLI default, D-024); PKCE with a loopback `streamboat://`
//! redirect arrives with the desktop shell.

pub mod device_code;

/// Requested once and never changed: TIDAL's SDK logs users out when the
/// scope set of a refresh differs from the minted token's (`tidal-api` auth §12).
pub const SCOPES: &str = "r_usr w_usr w_sub";
