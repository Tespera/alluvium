//! Anthropic API key storage via OS Keychain.
//!
//! Wraps the `keyring` crate. On macOS the entry lives in the user's login
//! keychain under service `"alluvium"` / account `"anthropic-api-key"`.
//! On Linux it uses libsecret/SecretService; on Windows the Credential
//! Manager.
//!
//! ## Test caveat
//!
//! Linux CI runners (GitHub Actions Ubuntu) do not have a running
//! D-Bus / SecretService by default, so any test that touches
//! `keyring::Entry` would fail there. Therefore the keyring-touching
//! tests are gated behind `#[cfg(target_os = "macos")]` — they run
//! locally on the developer's Mac and on the macOS CI runner, where
//! Keychain Services is always available. The non-Keychain logic
//! (env-var fallback) is tested everywhere.

use anyhow::{Context, Result};

const SERVICE: &str = "alluvium";
const ACCOUNT: &str = "anthropic-api-key";
const ENV_FALLBACK: &str = "ANTHROPIC_API_KEY";

/// Return the API key, preferring the OS Keychain over `$ANTHROPIC_API_KEY`.
///
/// Resolution order:
///   1. `$ANTHROPIC_API_KEY` env var (if non-empty) — useful for CI / dev
///   2. OS Keychain
///
/// Env var first deliberately: lets a dev override without touching the
/// keychain entry. `alluvium init` writes to the keychain; this function
/// reads from either source.
pub fn get_api_key() -> Result<String> {
    if let Ok(key) = std::env::var(ENV_FALLBACK) {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    let entry = keyring::Entry::new(SERVICE, ACCOUNT).context("constructing keyring entry")?;
    entry
        .get_password()
        .with_context(|| format!(
            "no API key found — set ${ENV_FALLBACK} or run `alluvium init` to store one in the OS keychain"
        ))
}

/// Persist the API key to the OS Keychain. Overwrites any existing entry.
pub fn set_api_key(key: &str) -> Result<()> {
    if key.is_empty() {
        anyhow::bail!("refusing to store empty API key");
    }
    let entry = keyring::Entry::new(SERVICE, ACCOUNT).context("constructing keyring entry")?;
    entry
        .set_password(key)
        .context("writing API key to OS keychain")
}

/// Remove the stored API key (best-effort; missing entry is not an error).
pub fn delete_api_key() -> Result<()> {
    let entry = match keyring::Entry::new(SERVICE, ACCOUNT) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("deleting keychain entry: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env-var fallback works on every platform (no keychain dependency).
    /// Use `temp_env` would be cleaner, but a simple set/restore works for
    /// a single-threaded test.
    #[test]
    fn env_var_fallback_returns_value_when_set() {
        let prev = std::env::var(ENV_FALLBACK).ok();
        // SAFETY: tests run single-threaded enough that this is fine.
        unsafe {
            std::env::set_var(ENV_FALLBACK, "test-key-from-env");
        }
        let result = get_api_key();
        // Restore env first so a later assert failure doesn't leak.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(ENV_FALLBACK, v),
                None => std::env::remove_var(ENV_FALLBACK),
            }
        }
        let key = result.unwrap();
        assert_eq!(key, "test-key-from-env");
    }

    #[test]
    fn empty_env_var_is_treated_as_unset() {
        let prev = std::env::var(ENV_FALLBACK).ok();
        unsafe {
            std::env::set_var(ENV_FALLBACK, "");
        }
        // Try to get key. Should fall through to keyring (which may or may not
        // have an entry). We just verify env="" doesn't return "" successfully.
        let result = get_api_key();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(ENV_FALLBACK, v),
                None => std::env::remove_var(ENV_FALLBACK),
            }
        }
        // If keyring has no entry, this is an error; if it does, the value
        // is non-empty. Either way, an empty-string success would be a bug.
        if let Ok(k) = result {
            assert!(!k.is_empty(), "empty env var must not surface as empty key");
        }
    }

    #[test]
    fn set_empty_key_rejected() {
        let err = set_api_key("").unwrap_err();
        assert!(format!("{err:#}").contains("empty"));
    }

    /// macOS-only: full round-trip through Keychain Services.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_keychain_round_trip() {
        // Use a unique account string so we don't clobber the user's real
        // entry while testing on their dev machine.
        let test_account = format!("anthropic-api-key-test-{}", std::process::id());
        let entry = keyring::Entry::new(SERVICE, &test_account).unwrap();

        let test_key = "sk-ant-test-1234567890";
        entry.set_password(test_key).unwrap();
        let got = entry.get_password().unwrap();
        assert_eq!(got, test_key);

        // Cleanup.
        let _ = entry.delete_credential();
    }
}
