//! Anthropic API key storage via OS Keychain.
//!
//! Uses the `keyring` crate. On macOS the entry lives in the user's login
//! keychain under service "alluvium" / account "anthropic-api-key".

use anyhow::Result;

const SERVICE: &str = "alluvium";
const ACCOUNT: &str = "anthropic-api-key";

pub fn get_api_key() -> Result<String> {
    let _ = (SERVICE, ACCOUNT);
    anyhow::bail!("config::secrets::get_api_key: not yet implemented (scaffold v0.1)")
}

pub fn set_api_key(_key: &str) -> Result<()> {
    anyhow::bail!("config::secrets::set_api_key: not yet implemented (scaffold v0.1)")
}
