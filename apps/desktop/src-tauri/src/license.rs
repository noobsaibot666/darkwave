// Direct-sale licensing: HWID-bound Ed25519 activation tokens plus a
// one-time local trial, modeled on the same server-side contract already
// proven in production by exposeu_wrapkit's src-tauri/src/license.rs
// (signed_data = "{key}:{hwid}:{expires_at}", 14-day trial, 2-device cap
// enforced server-side). Entirely absent from the Mac App Store build:
// every command below is `direct-dist`-gated, and the fallback half of
// this file (bottom) stubs the same command surface to always report an
// active license with zero crypto/network/keyring code compiled in —
// Apple's IAP-only payment rule is trivially satisfied because there is
// nothing here to violate it.
//
// One deliberate improvement over the reference implementation: the local
// token/trial state is stored in the OS keychain (via the `keyring` crate)
// rather than a flat XOR-"obfuscated" JSON file — exposeu_wrapkit's own
// docs describe that XOR step as "not cryptographically sealed," i.e. a
// speed bump, not protection. Keychain storage costs the same amount of
// code and is the real thing.

use serde::{Deserialize, Serialize};

// Same shared endpoints CineFlow already uses (/activate, /resend-key —
// see licensing-server/server.js) — product is resolved server-side from
// the license row itself, not from the URL, so this deliberately isn't
// "/licensing/darkwave".
#[cfg(feature = "direct-dist")]
const LICENSE_SERVER_URL: &str = "https://alan-design.com/licensing";

// Raw 32-byte Ed25519 public key, base64-encoded — must be exactly 32 bytes
// once decoded, since ed25519-dalek's VerifyingKey::try_from requires the
// bare key, not an SPKI-wrapped one (an SPKI export, e.g. from Node's
// `publicKey.export({format:'der',type:'spki'})`, is 44 bytes with a
// 12-byte ASN.1 header and would fail try_into::<[u8;32]>() every time —
// this is a real bug found in exposeu_wrapkit's own license.rs while
// building this, not a hypothetical; that one embeds an SPKI-formatted key,
// so its verify_signature() should always return false in practice. Worth
// fixing there separately — not done here since that's a different,
// already-shipping product. Generate this key's raw form the same way this
// one was: `publicKey.export({format:'jwk'})`, base64url-decode `.x`, then
// base64-encode those 32 bytes.
//
// The matching private key (PKCS8 DER, base64) lives in
// secrets/darkwave-license-signing.key, to be set as
// DARKWAVE_LICENSE_PRIVATE_KEY_B64 in the licensing server's environment —
// distinct from secrets/darkwave-updater.key (Phase D's update-signing key,
// a different purpose entirely).
#[cfg(feature = "direct-dist")]
const PUBLIC_KEY_B64: &str = "28F/mm6cMaNzARkb0rmg9LLA+fGLEkz2bq45iFsWM0Y=";

#[cfg(feature = "direct-dist")]
const TRIAL_DURATION_DAYS: i64 = 14;

#[cfg(feature = "direct-dist")]
const KEYRING_SERVICE: &str = "dev.darkwave.app";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LicenseStatus {
    pub active: bool,
    pub key: Option<String>,
    pub hwid: String,
    pub message: Option<String>,
    pub is_trial: bool,
    pub trial_days_remaining: Option<i64>,
    pub trial_expired: bool,
}

#[derive(Debug, Serialize)]
pub struct LicenseRecoveryStatus {
    pub message: String,
}

#[cfg(feature = "direct-dist")]
#[derive(Debug, Serialize, Deserialize)]
struct TrialState {
    started_at: i64,
    hwid: String,
}

#[cfg(feature = "direct-dist")]
#[derive(Debug, Serialize, Deserialize)]
struct ActivationToken {
    key: String,
    hwid: String,
    expires_at: i64,
    signature: String,
}

#[cfg(feature = "direct-dist")]
#[tauri::command]
pub fn get_hwid() -> String {
    machine_uid::get().unwrap_or_else(|_| "unknown_hwid".to_string())
}

#[cfg(feature = "direct-dist")]
fn keyring_entry(username: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, username).map_err(|error| error.to_string())
}

#[cfg(feature = "direct-dist")]
fn read_keyring_json<T: serde::de::DeserializeOwned>(username: &str) -> Option<T> {
    let entry = keyring_entry(username).ok()?;
    let raw = entry.get_password().ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(feature = "direct-dist")]
fn write_keyring_json<T: Serialize>(username: &str, value: &T) -> Result<(), String> {
    let entry = keyring_entry(username)?;
    let raw = serde_json::to_string(value).map_err(|error| error.to_string())?;
    entry.set_password(&raw).map_err(|error| error.to_string())
}

#[cfg(feature = "direct-dist")]
fn check_trial_status(hwid: &str) -> Option<LicenseStatus> {
    let trial: TrialState = read_keyring_json("trial")?;

    if trial.hwid != hwid {
        // Deliberately not a trial grant: copying trial state to another
        // machine (there is none to copy — the keychain entry itself
        // doesn't roam — but a HWID mismatch here would mean the OS
        // reported a different id on a second read) must not silently
        // restart the clock.
        return Some(LicenseStatus {
            active: false,
            key: None,
            hwid: hwid.to_string(),
            message: Some("No license found".into()),
            is_trial: false,
            trial_days_remaining: None,
            trial_expired: false,
        });
    }

    let now = chrono::Utc::now().timestamp();
    let elapsed_days = (now - trial.started_at) / 86_400;
    let remaining = TRIAL_DURATION_DAYS - elapsed_days;

    if remaining > 0 {
        Some(LicenseStatus {
            active: false,
            key: None,
            hwid: hwid.to_string(),
            message: None,
            is_trial: true,
            trial_days_remaining: Some(remaining),
            trial_expired: false,
        })
    } else {
        Some(LicenseStatus {
            active: false,
            key: None,
            hwid: hwid.to_string(),
            message: Some("Trial expired".into()),
            is_trial: false,
            trial_days_remaining: Some(0),
            trial_expired: true,
        })
    }
}

#[cfg(feature = "direct-dist")]
pub fn check_license() -> LicenseStatus {
    let hwid = get_hwid();

    if let Some(token) = read_keyring_json::<ActivationToken>("license") {
        if token.hwid != hwid {
            return LicenseStatus {
                active: false,
                key: Some(token.key),
                hwid,
                message: Some("Hardware mismatch".into()),
                is_trial: false,
                trial_days_remaining: None,
                trial_expired: false,
            };
        }

        let now = chrono::Utc::now().timestamp();
        if token.expires_at < now {
            return LicenseStatus {
                active: false,
                key: Some(token.key),
                hwid,
                message: Some("License expired".into()),
                is_trial: false,
                trial_days_remaining: None,
                trial_expired: false,
            };
        }

        if verify_signature(&token) {
            return LicenseStatus {
                active: true,
                key: Some(token.key),
                hwid,
                message: None,
                is_trial: false,
                trial_days_remaining: None,
                trial_expired: false,
            };
        }

        if let Some(trial_status) = check_trial_status(&hwid) {
            return trial_status;
        }

        return LicenseStatus {
            active: false,
            key: Some(token.key),
            hwid,
            message: Some("Invalid signature".into()),
            is_trial: false,
            trial_days_remaining: None,
            trial_expired: false,
        };
    }

    if let Some(trial_status) = check_trial_status(&hwid) {
        return trial_status;
    }

    LicenseStatus {
        active: false,
        key: None,
        hwid,
        message: Some("No license found".into()),
        is_trial: false,
        trial_days_remaining: None,
        trial_expired: false,
    }
}

#[cfg(feature = "direct-dist")]
fn verify_signature(token: &ActivationToken) -> bool {
    use base64::{engine::general_purpose, Engine as _};
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let public_key_bytes = match general_purpose::STANDARD.decode(PUBLIC_KEY_B64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let verifying_key = match VerifyingKey::try_from(public_key_bytes.as_slice()) {
        Ok(key) => key,
        Err(_) => return false,
    };
    let signature_bytes = match general_purpose::STANDARD.decode(&token.signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature = match Signature::from_slice(signature_bytes.as_slice()) {
        Ok(signature) => signature,
        Err(_) => return false,
    };

    // Must match the exact string the server signs — see licensing-server's
    // signToken() (web_three/licensing-server/server.js) once Phase G
    // generalizes it for Darkwave.
    let signed_data = format!("{}:{}:{}", token.key, token.hwid, token.expires_at);
    verifying_key.verify(signed_data.as_bytes(), &signature).is_ok()
}

#[cfg(feature = "direct-dist")]
#[tauri::command]
pub async fn activate_license(key: String, email: String) -> Result<LicenseStatus, String> {
    let hwid = get_hwid();
    let client = reqwest::Client::new();
    let clean_key = key.trim().to_uppercase();
    let clean_email = email.trim().to_lowercase();

    let response = client
        .post(format!("{LICENSE_SERVER_URL}/activate"))
        .json(&serde_json::json!({
            "key": clean_key,
            "email": clean_email,
            "hwid": hwid,
            "product": "darkwave",
        }))
        .send()
        .await
        .map_err(|error| format!("Network error: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Could not read error response".to_string());
        return Err(summarize_activation_error(status, &body));
    }

    let token: ActivationToken = response
        .json()
        .await
        .map_err(|error| format!("Invalid response from server: {error}"))?;

    write_keyring_json("license", &token)?;

    Ok(LicenseStatus {
        active: true,
        key: Some(token.key),
        hwid,
        message: None,
        is_trial: false,
        trial_days_remaining: None,
        trial_expired: false,
    })
}

#[cfg(feature = "direct-dist")]
fn summarize_activation_error(status: reqwest::StatusCode, body: &str) -> String {
    if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = json_error["error"].as_str() {
            return message.to_string();
        }
    }
    if status.is_client_error() || status.is_server_error() {
        format!("Server error ({status}): {}", summarize_server_error(body))
    } else {
        "Activation failed: unknown server response".to_string()
    }
}

#[cfg(feature = "direct-dist")]
fn summarize_server_error(body: &str) -> String {
    if body.contains("<html") || body.contains("<!DOCTYPE html") {
        return "The licensing server returned an HTML error page.".into();
    }
    body.trim().chars().take(240).collect()
}

#[cfg(feature = "direct-dist")]
#[tauri::command]
pub async fn recover_license_key(email: String) -> Result<LicenseRecoveryStatus, String> {
    let clean_email = email.trim().to_lowercase();
    if clean_email.is_empty() {
        return Err("Please enter your email first to recover your key.".into());
    }

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{LICENSE_SERVER_URL}/resend-key"))
        // Unlike /activate (which can derive product from the matched
        // license row), the server has no key to look up here — that's
        // the whole point of key recovery. Sending "product" lets it
        // filter to the right one for a customer who's bought more than
        // one product with this email, instead of resending whichever
        // license happens to be newest overall.
        .json(&serde_json::json!({ "email": clean_email, "product": "darkwave" }))
        .send()
        .await
        .map_err(|error| format!("Network error: {error}"))?;

    if response.status().is_success() {
        Ok(LicenseRecoveryStatus {
            message: "License key recovery sent to your email.".into(),
        })
    } else {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Could not read error response".to_string());
        Err(summarize_activation_error(status, &body))
    }
}

#[cfg(feature = "direct-dist")]
#[tauri::command]
pub fn get_license_status() -> LicenseStatus {
    check_license()
}

#[cfg(feature = "direct-dist")]
#[tauri::command]
pub fn init_trial() -> Result<LicenseStatus, String> {
    // Idempotent: only ever write the trial start timestamp once. This is
    // the sole anti-reset mechanism — deleting the keychain entry resets
    // the trial, same accepted trade-off exposeu_wrapkit's docs describe
    // ("a business mechanism, not a security boundary").
    if read_keyring_json::<TrialState>("trial").is_none() {
        let trial = TrialState {
            started_at: chrono::Utc::now().timestamp(),
            hwid: get_hwid(),
        };
        write_keyring_json("trial", &trial)?;
    }
    Ok(check_license())
}

#[cfg(not(feature = "direct-dist"))]
#[tauri::command]
pub fn get_hwid() -> String {
    String::new()
}

#[cfg(not(feature = "direct-dist"))]
pub fn check_license() -> LicenseStatus {
    LicenseStatus {
        active: true,
        key: None,
        hwid: String::new(),
        message: None,
        is_trial: false,
        trial_days_remaining: None,
        trial_expired: false,
    }
}

#[cfg(not(feature = "direct-dist"))]
#[tauri::command]
pub fn get_license_status() -> LicenseStatus {
    check_license()
}

#[cfg(not(feature = "direct-dist"))]
#[tauri::command]
pub async fn activate_license(_key: String, _email: String) -> Result<LicenseStatus, String> {
    Err("Licensing not enabled in this build".into())
}

#[cfg(not(feature = "direct-dist"))]
#[tauri::command]
pub async fn recover_license_key(_email: String) -> Result<LicenseRecoveryStatus, String> {
    Err("Licensing not enabled in this build".into())
}

#[cfg(not(feature = "direct-dist"))]
#[tauri::command]
pub fn init_trial() -> Result<LicenseStatus, String> {
    Ok(check_license())
}
