//! Settings → Providers: every built-in provider from [`crate::catalog`], the
//! config switch that turns one on or off, and the action that signs it in.
//!
//! The report only carries enabled providers, so a popover that lists its
//! cards cannot show — or enable — the ones that are off. This is the row
//! source for those, kept in Rust so the popover holds no provider table.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::payload::{KeyCheckFact, KeyCheckState, SharedFacts, with_facts};
use crate::catalog::VendorStatus;
use crate::vendor::{AuthKind, VendorId};

/// What "Sign in" does for one provider on this host.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignIn {
    /// Run the provider's own login command in a terminal.
    Terminal(&'static str),
    /// Bring a desktop app forward; its window is where the login happens.
    App(&'static str),
    /// Key providers: the TUI's Settings overlay is where a key goes.
    Settings,
    None,
}

impl SignIn {
    fn kind(self) -> &'static str {
        match self {
            SignIn::Terminal(_) => "terminal",
            SignIn::App(_) => "app",
            SignIn::Settings => "settings",
            SignIn::None => "",
        }
    }
}

/// `macos` selects the hosts that can open an app by name (`open -a`).
pub(crate) fn sign_in_for(vendor: VendorId, macos: bool) -> SignIn {
    match vendor {
        VendorId::Cursor if macos => SignIn::App("Cursor"),
        VendorId::Antigravity if macos => SignIn::App("Antigravity"),
        _ if !vendor.login_command().is_empty() => SignIn::Terminal(vendor.login_command()),
        _ if vendor.auth_kind() == AuthKind::ApiKey => SignIn::Settings,
        _ => SignIn::None,
    }
}

/// The `providers` array of the popover payload.
pub(crate) fn rows_json(rows: &[VendorStatus], macos: bool) -> Value {
    Value::Array(
        rows.iter()
            .filter_map(|row| {
                let vendor = VendorId::from_slug(row.id)?;
                let mut entry = json!({
                    "id": row.id,
                    "name": row.name,
                    "kind": row.kind.as_str(),
                    "enabled": row.enabled,
                    "configured": row.configured,
                    "env": row.env,
                    "sign_in": vendor.sign_in_hint(),
                    "action": sign_in_for(vendor, macos).kind(),
                    "add_accounts": add_account_kinds(vendor, macos),
                });
                // The key dialog's fields, straight from the settings
                // overlay's own table: the popover keeps no credential
                // labels of its own, and only key vendors get the pair.
                if let (Some(key), Some(obj)) = (
                    crate::tui::settings::key_vendor_for_slug(row.id),
                    entry.as_object_mut(),
                ) {
                    obj.insert("key_label".into(), json!(key.secret_label));
                    obj.insert("key_note".into(), json!(key.note));
                }
                Some(entry)
            })
            .collect(),
    )
}

static DUE: AtomicBool = AtomicBool::new(true);

/// Ask the next report to re-read the catalog. It is not read on every poll:
/// on macOS it consults the Keychain once per Claude account.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn request() {
    DUE.store(true, Ordering::Relaxed);
}

/// The fresh catalog when one was requested (and at startup), else `None`.
/// An unreadable config lists every provider against the defaults.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn take_due() -> Option<Vec<VendorStatus>> {
    DUE.swap(false, Ordering::Relaxed)
        .then(|| crate::catalog::statuses(&crate::config::Config::load().unwrap_or_default()))
}

/// `{"cmd":"set-provider-enabled","id":"cursor","value":true}`: flip the
/// provider's `enabled` in config.toml. The slug is validated by
/// [`crate::config::set_vendor_enabled`], so no other section is reachable.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn set_enabled(value: &Value) -> Result<(), String> {
    let (Some(id), Some(enabled)) = (
        value.get("id").and_then(Value::as_str),
        value.get("value").and_then(Value::as_bool),
    ) else {
        return Err("Malformed provider request.".into());
    };
    let path = crate::config::resolved_path()
        .ok_or_else(|| "Could not resolve the config.toml path.".to_string())?;
    crate::config::set_vendor_enabled(&path, id, enabled).map_err(|error| error.user_message())
}

/// `{"cmd":"sign-in-provider","id":"cursor"}`.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn sign_in(value: &Value) {
    let Some(vendor) = value
        .get("id")
        .and_then(Value::as_str)
        .and_then(VendorId::from_slug)
    else {
        return;
    };
    #[cfg(any(windows, target_os = "macos"))]
    match sign_in_for(vendor, cfg!(target_os = "macos")) {
        SignIn::Terminal(command) => super::tui_launch::run_in_terminal(&login_line(command)),
        SignIn::App(name) => open_app(name),
        SignIn::Settings => super::tui_launch::open(),
        SignIn::None => {}
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = vendor;
}

/// `{"cmd":"add-account","kind":"code"|"desktop"|"codex"}`: register a new
/// Claude Code, Claude Desktop or Codex account in Terminal, which asks for
/// the label and runs the interactive sign-in `account add` starts.
#[cfg(target_os = "macos")]
pub(crate) fn add_account(value: &Value) {
    let Some(flag) = value
        .get("kind")
        .and_then(Value::as_str)
        .and_then(add_account_flag)
    else {
        return;
    };
    let bin = super::tui_launch::quote(&super::tui_launch::sibling("ai-usagebar"));
    super::tui_launch::run_in_terminal(&format!(
        "read -r -p 'New account label: ' label && {bin} account add \"$label\"{flag}"
    ));
}

/// Which `add-account` kinds a provider's row offers. Account capture runs
/// through the macOS host only, like the account switch.
fn add_account_kinds(vendor: VendorId, macos: bool) -> &'static [&'static str] {
    match vendor {
        VendorId::Anthropic if macos => &["code", "desktop"],
        VendorId::Openai if macos => &["codex"],
        _ => &[],
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn add_account_flag(kind: &str) -> Option<&'static str> {
    match kind {
        "code" => Some(""),
        "desktop" => Some(" --desktop"),
        "codex" => Some(" --codex"),
        _ => None,
    }
}

/// `ai-usagebar auth …` names this project's own CLI, which a tray-only
/// install may not have on `PATH`; point it at the binary beside the tray.
#[cfg(any(windows, target_os = "macos"))]
fn login_line(command: &str) -> String {
    match command.strip_prefix("ai-usagebar ") {
        Some(rest) => format!(
            "{} {rest}",
            super::tui_launch::quote(&super::tui_launch::sibling("ai-usagebar"))
        ),
        None => command.to_string(),
    }
}

/// `{"cmd":"set-api-key","id":"zai","key":"…","seq":17}`: the popover's
/// key dialog. The key is written with the settings overlay's writer, the
/// provider is fetched alone, and the verdict is published as the
/// `key_check` fact — a refusal restores the previous value. The key never
/// appears in a fact, payload, error, log, or thread name.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn request_key_check(
    facts: &SharedFacts,
    value: &Value,
    notify: impl FnOnce() + Send + 'static,
) -> std::result::Result<Option<std::thread::JoinHandle<()>>, KeyCheckFact> {
    request_key_check_with(facts, value, fetch_filtered_report, notify)
}

/// The production fetch: one provider's report through the same entry point
/// the tray's report uses, on a current-thread runtime like the account
/// prepare. Tests pass a fake so a check never touches the wire.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn fetch_filtered_report(slug: &str) -> std::result::Result<String, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .and_then(|runtime| {
            runtime
                .block_on(crate::report::collect_filtered_json(Some(slug), None))
                .ok()
        })
        .ok_or_else(|| "Could not start the key check worker.".to_string())
}

/// The shared check: parse the request, publish `Checking`, run the
/// write — fetch — restore dance on its own thread. `fetch` and `notify`
/// are the seams the hosts and the tests plug in, so the thread never names
/// either one. A malformed request — a missing field, a non-string slug or
/// key, a `seq` that is not a `u64`, a vendor outside `KEY_VENDORS`, or a
/// key the overlay would refuse — is ignored: `Ok(None)`, no response, no
/// write. The error is the verdict the host publishes when the thread
/// cannot even be started.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn request_key_check_with(
    facts: &SharedFacts,
    value: &Value,
    fetch: impl FnOnce(&str) -> std::result::Result<String, String> + Send + 'static,
    notify: impl FnOnce() + Send + 'static,
) -> std::result::Result<Option<std::thread::JoinHandle<()>>, KeyCheckFact> {
    let Some((slug, key, seq)) = key_check_request(value) else {
        return Ok(None);
    };
    let Some(vendor) = crate::tui::settings::key_vendor_for_slug(&slug) else {
        return Ok(None);
    };
    if crate::tui::settings::validate_key_value(vendor, &key).is_err() {
        return Ok(None);
    }
    // The path is resolved here, before the thread, so the thread's
    // read — write — restore trio all touch the one file, and the thread
    // itself never reads process-global state (tests steer it with the
    // `--config` override seam).
    let path = crate::config::resolved_path().ok_or_else(|| KeyCheckFact {
        id: slug.clone(),
        seq,
        state: KeyCheckState::Failed,
        error: "Could not resolve the config.toml path.".into(),
    })?;
    with_facts(facts, |f| {
        f.key_check = Some(KeyCheckFact {
            id: slug.clone(),
            seq,
            state: KeyCheckState::Checking,
            error: String::new(),
        });
    });
    let facts = Arc::clone(facts);
    let slug = slug.clone();
    let spawned_slug = slug.clone();
    std::thread::Builder::new()
        .name("ai-usagebar-key-check".into())
        .spawn(move || run_key_check(&facts, &path, &spawned_slug, key, seq, fetch, notify))
        .map(Some)
        .map_err(|_| KeyCheckFact {
            id: slug,
            seq,
            state: KeyCheckState::Failed,
            error: "Could not start the key check worker.".into(),
        })
}

/// The request's (vendor slug, key, sequence number), or `None` when the
/// body is not a well-formed `set-api-key`.
fn key_check_request(value: &Value) -> Option<(String, String, u64)> {
    Some((
        value.get("id").and_then(Value::as_str)?.to_string(),
        value.get("key").and_then(Value::as_str)?.to_string(),
        value.get("seq").and_then(Value::as_u64)?,
    ))
}

/// The check's thread: remember what the config holds, write the new key
/// with the overlay's writer, fetch the provider alone, and — on a refusal
/// — put the previous value back. The key is dropped as soon as the write
/// succeeds, so nothing after the write may name it.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn run_key_check(
    facts: &SharedFacts,
    path: &std::path::Path,
    slug: &str,
    key: String,
    seq: u64,
    fetch: impl FnOnce(&str) -> std::result::Result<String, String>,
    notify: impl FnOnce(),
) {
    let Some(vendor) = crate::tui::settings::key_vendor_for_slug(slug) else {
        return;
    };
    let previous = match crate::tui::settings::read_inline_key(path, vendor) {
        Ok(previous) => previous,
        Err(error) => {
            finish_key_check(
                facts,
                slug,
                seq,
                KeyCheckState::Failed,
                error.user_message(),
                notify,
            );
            return;
        }
    };
    if let Err(error) = crate::tui::settings::write_inline_key(path, vendor, &key) {
        finish_key_check(
            facts,
            slug,
            seq,
            KeyCheckState::Failed,
            error.user_message(),
            notify,
        );
        return;
    }
    // The credential's last reference: the fetch and the verdict below must
    // not be able to name it.
    drop(key);
    let (state, mut error) = key_check_result(slug, fetch(slug));
    if state == KeyCheckState::Failed {
        let restore = match previous.as_deref() {
            Some(value) => crate::tui::settings::restore_inline_key(path, vendor, Some(value)),
            None => crate::tui::settings::restore_inline_key(path, vendor, None),
        };
        if restore.is_err() {
            error.push_str(" The new key stayed in config.toml.");
        }
    }
    // The catalog changed (`configured`), and the report must re-read the
    // config either way: the key kept, or the value restored.
    request();
    finish_key_check(facts, slug, seq, state, error, notify);
}

/// Publish the verdict and ask the host for a fresh facts stamp and report.
/// The fact is written only when this check is still the newest one: a
/// newer send has already replaced it, and this thread's verdict belongs to
/// a send the popover no longer shows — the catalog and report refresh
/// still run, since this thread may have restored the config.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn finish_key_check(
    facts: &SharedFacts,
    slug: &str,
    seq: u64,
    state: KeyCheckState,
    error: String,
    notify: impl FnOnce(),
) {
    let newest = facts
        .lock()
        .map(|guard| guard.key_check.as_ref().is_some_and(|fact| fact.seq == seq))
        .unwrap_or(false);
    if newest {
        with_facts(facts, |f| {
            f.key_check = Some(KeyCheckFact {
                id: slug.to_string(),
                seq,
                state,
                error,
            });
        });
    }
    request();
    notify();
}

/// The verdict for one check's report. `Ok` only for this provider's entry
/// — `"status":"ready"` with `"stale":false`; every other shape is `Failed`,
/// with the entry's error, the fetch's error, or the fixed "no reading"
/// note when the report has no entry for the provider. The error is cut at
/// 300 characters, since it lands on one line.
pub(crate) fn key_check_result(
    slug: &str,
    report: std::result::Result<String, String>,
) -> (KeyCheckState, String) {
    const NO_READING: &str = "The provider returned no reading.";
    let (state, error) = match report {
        Err(error) => (KeyCheckState::Failed, error),
        Ok(document) => match serde_json::from_str::<Value>(&document) {
            Err(_) => (KeyCheckState::Failed, NO_READING.to_string()),
            Ok(value) => {
                let entries: Vec<&Value> = value
                    .get("entries")
                    .and_then(Value::as_array)
                    .map_or_else(Vec::new, |list| list.iter().collect());
                let mine: Vec<&Value> = entries
                    .iter()
                    .filter(|entry| is_provider_entry(entry, slug))
                    .copied()
                    .collect();
                if mine.iter().any(|entry| {
                    entry.get("status").and_then(Value::as_str) == Some("ready")
                        && entry.get("stale").and_then(Value::as_bool) == Some(false)
                }) {
                    (KeyCheckState::Ok, String::new())
                } else if let Some(entry) = mine.first() {
                    (
                        KeyCheckState::Failed,
                        entry
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                } else {
                    (KeyCheckState::Failed, NO_READING.to_string())
                }
            }
        },
    };
    (state, cut_key_check_error(&error))
}

/// The report's entry for the check's provider: its own slug, or a
/// `slug@account` id, which a multi-account vendor would still name.
fn is_provider_entry(entry: &Value, slug: &str) -> bool {
    entry.get("id").and_then(Value::as_str).is_some_and(|id| {
        id == slug
            || id
                .strip_prefix(slug)
                .is_some_and(|rest| rest.starts_with('@'))
    })
}

/// The verdict's error line, cut at 300 characters on a char boundary.
fn cut_key_check_error(error: &str) -> String {
    error.chars().take(300).collect()
}

#[cfg(target_os = "macos")]
fn open_app(name: &str) {
    let _ = std::process::Command::new("/usr/bin/open")
        .args(["-a", name])
        .spawn();
}

#[cfg(windows)]
fn open_app(_name: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(windows, target_os = "macos"))]
    use crate::tray::payload::{facts_snapshot, wrap_report};

    #[test]
    fn every_provider_gets_an_action_or_says_it_has_none() {
        for &vendor in VendorId::all() {
            let action = sign_in_for(vendor, true);
            match vendor.auth_kind() {
                AuthKind::Oauth => assert!(matches!(action, SignIn::Terminal(_)), "{vendor:?}"),
                AuthKind::ApiKey if vendor.login_command().is_empty() => {
                    assert_eq!(action, SignIn::Settings, "{vendor:?}")
                }
                _ => {}
            }
        }
    }

    #[test]
    fn desktop_apps_open_only_where_open_a_exists() {
        assert_eq!(sign_in_for(VendorId::Cursor, true), SignIn::App("Cursor"));
        assert_eq!(sign_in_for(VendorId::Cursor, false), SignIn::None);
        assert_eq!(
            sign_in_for(VendorId::Openai, false),
            SignIn::Terminal("codex login")
        );
        assert_eq!(sign_in_for(VendorId::Zai, false), SignIn::Settings);
    }

    #[test]
    fn rows_carry_every_vendor_with_hint_and_action() {
        let rows = crate::catalog::statuses_with(
            &crate::config::Config::default(),
            &crate::catalog::Probes {
                env_set: &|_| false,
                exists: &|_| false,
                keychain_has_claude: &|| false,
                keychain_has_claude_for: &|_| false,
                commandcode_signed_in: &|_| false,
                opencode_go_signed_in: &|_| false,
                cline_signed_in: &|_| false,
            },
        );
        let json = rows_json(&rows, true);
        let list = json.as_array().unwrap();
        assert_eq!(list.len(), VendorId::all().len());
        let cursor = list.iter().find(|row| row["id"] == "cursor").unwrap();
        assert_eq!(cursor["enabled"], false);
        assert_eq!(cursor["configured"], false);
        assert_eq!(cursor["action"], "app");
        assert_eq!(cursor["sign_in"], VendorId::Cursor.sign_in_hint());
    }

    #[test]
    fn add_account_accepts_only_the_three_known_kinds() {
        assert_eq!(add_account_flag("code"), Some(""));
        assert_eq!(add_account_flag("desktop"), Some(" --desktop"));
        assert_eq!(add_account_flag("codex"), Some(" --codex"));
        assert_eq!(add_account_flag("; rm -rf ~"), None);
        for &vendor in VendorId::all() {
            assert!(add_account_kinds(vendor, false).is_empty());
            for kind in add_account_kinds(vendor, true) {
                assert!(add_account_flag(kind).is_some(), "{kind}");
            }
        }
    }

    #[test]
    fn malformed_enable_requests_are_refused_before_touching_config() {
        assert!(set_enabled(&json!({"id": "cursor"})).is_err());
        assert!(set_enabled(&json!({"value": true})).is_err());
    }

    #[test]
    fn rows_carry_the_key_vendors_credential_fields_only() {
        let rows = crate::catalog::statuses_with(
            &crate::config::Config::default(),
            &crate::catalog::Probes {
                env_set: &|_| false,
                exists: &|_| false,
                keychain_has_claude: &|| false,
                keychain_has_claude_for: &|_| false,
                commandcode_signed_in: &|_| false,
                opencode_go_signed_in: &|_| false,
                cline_signed_in: &|_| false,
            },
        );
        let json = rows_json(&rows, true);
        let list = json.as_array().unwrap();
        // Every key vendor carries the overlay's own label and note...
        for vendor in crate::tui::settings::KEY_VENDORS {
            let row = list
                .iter()
                .find(|row| row["id"] == vendor.id.slug())
                .unwrap();
            assert_eq!(
                row["key_label"],
                vendor.secret_label,
                "{}",
                vendor.id.slug()
            );
            assert_eq!(row["key_note"], vendor.note, "{}", vendor.id.slug());
        }
        // ...and no other vendor carries the pair at all.
        for &id in VendorId::all() {
            if crate::tui::settings::KEY_VENDORS
                .iter()
                .any(|kv| kv.id == id)
            {
                continue;
            }
            let row = list.iter().find(|row| row["id"] == id.slug()).unwrap();
            assert!(row.get("key_label").is_none(), "{:?}", id);
            assert!(row.get("key_note").is_none(), "{:?}", id);
        }
    }

    #[test]
    fn key_check_requests_parse_or_are_none() {
        assert_eq!(
            key_check_request(&json!({"id": "zai", "key": "test-key-value", "seq": 7})),
            Some(("zai".to_string(), "test-key-value".to_string(), 7))
        );
        // Missing or non-`u64` seq: ignored.
        assert_eq!(
            key_check_request(&json!({"id": "zai", "key": "test-key-value"})),
            None
        );
        assert_eq!(
            key_check_request(&json!({"id": "zai", "key": "test-key-value", "seq": 1.5})),
            None
        );
        assert_eq!(
            key_check_request(&json!({"id": "zai", "key": "test-key-value", "seq": "7"})),
            None
        );
        assert_eq!(
            key_check_request(&json!({"id": "zai", "key": "test-key-value", "seq": -1})),
            None
        );
        // Non-string key or id: ignored.
        assert_eq!(
            key_check_request(&json!({"id": "zai", "key": 7, "seq": 1})),
            None
        );
        assert_eq!(
            key_check_request(&json!({"id": 7, "key": "test-key-value", "seq": 1})),
            None
        );
    }

    #[cfg(any(windows, target_os = "macos"))]
    /// Serializes the tests that steer the process-wide `--config` override
    /// at a temp file, and clears it when the test ends — including via a
    /// panic. A leaked override makes every later test resolve a deleted
    /// temp file, turning one failure into a cascade of sibling failures.
    struct ScopedKeyOverride {
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    #[cfg(any(windows, target_os = "macos"))]
    impl Drop for ScopedKeyOverride {
        fn drop(&mut self) {
            crate::config::clear_override_path();
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn override_guard() -> ScopedKeyOverride {
        static M: std::sync::Mutex<()> = std::sync::Mutex::new(());
        ScopedKeyOverride {
            _serial: M.lock().unwrap_or_else(|p| p.into_inner()),
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_value(id: &str, key: &str, seq: u64) -> Value {
        json!({"cmd": "set-api-key", "id": id, "key": key, "seq": seq})
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn default_facts() -> SharedFacts {
        Arc::new(std::sync::Mutex::new(
            crate::tray::payload::HostFacts::default(),
        ))
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_ignores_malformed_requests_without_writing_or_answering() {
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        crate::config::set_override_path(&path);
        let facts = default_facts();
        let fetch_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // "anthropic" is a vendor, but an OAuth one: no credential field.
        for value in [
            json!({"cmd": "set-api-key", "id": "anthropic", "key": "test-key-value", "seq": 1}),
            json!({"cmd": "set-api-key", "id": "zai", "key": "test-key-value"}),
            json!({"cmd": "set-api-key", "id": "zai", "key": "test-key-value", "seq": 1.5}),
            json!({"cmd": "set-api-key", "id": "zai", "key": 7, "seq": 1}),
            // A key the overlay would refuse: no response, no write.
            json!({"cmd": "set-api-key", "id": "zai", "key": "a\u{1b}[31mb", "seq": 1}),
        ] {
            let called = Arc::clone(&fetch_called);
            let result = request_key_check_with(
                &facts,
                &value,
                move |slug| {
                    called.store(true, Ordering::Relaxed);
                    Ok::<_, String>(slug.to_string())
                },
                || {},
            )
            .expect("an ignored request is not a spawn failure");
            assert!(result.is_none(), "the request must be ignored: {value}");
        }
        // Nothing was published, nothing fetched, nothing written.
        assert!(facts_snapshot(&facts).key_check.is_none());
        assert!(!fetch_called.load(Ordering::Relaxed));
        assert!(
            !path.exists(),
            "an ignored request must not create the config"
        );
        drop(guard);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_failed_restores_the_previous_value_and_publishes() {
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original =
            "[ui]\nprimary = \"zai\"\n[zai]\napi_key = \"old-key-value\"\nenabled = true\n";
        std::fs::write(&path, original).unwrap();
        crate::config::set_override_path(&path);
        let facts = default_facts();
        // The catalog must be re-read by the check (`configured` changed).
        assert!(take_due().is_some());
        let handle = request_key_check_with(
            &facts,
            &key_check_value("zai", "test-key-value", 7),
            |slug| {
                assert_eq!(slug, "zai");
                Ok(r#"{"entries":[{"id":"zai","status":"error","stale":false,"error":"HTTP 401: invalid key"}]}"#.into())
            },
            || {},
        )
        .expect("spawn")
        .expect("a well-formed request starts the check");
        handle.join().unwrap();
        // The config is back exactly where the check found it...
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        // ...the verdict was published under the request's id and seq...
        let fact = facts_snapshot(&facts)
            .key_check
            .expect("the check must publish its verdict");
        assert_eq!(fact.id, "zai");
        assert_eq!(fact.seq, 7);
        assert_eq!(fact.state, KeyCheckState::Failed);
        assert_eq!(fact.error, "HTTP 401: invalid key");
        // ...and the catalog was re-requested.
        assert!(take_due().is_some());
        drop(guard);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_keeps_the_key_when_the_provider_accepts() {
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "[ui]\nprimary = \"zai\"\n";
        std::fs::write(&path, original).unwrap();
        crate::config::set_override_path(&path);
        let facts = default_facts();
        let handle = request_key_check_with(
            &facts,
            &key_check_value("zai", "test-key-value", 3),
            |_| Ok(r#"{"entries":[{"id":"zai","status":"ready","stale":false}]}"#.into()),
            || {},
        )
        .expect("spawn")
        .unwrap();
        handle.join().unwrap();
        // An accepted key stays in the config, with the vendor opted in.
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains(r#"api_key = "test-key-value""#),
            "{written}"
        );
        assert!(written.contains("enabled = true"), "{written}");
        let fact = facts_snapshot(&facts)
            .key_check
            .expect("the check must publish its verdict");
        assert_eq!(fact.state, KeyCheckState::Ok);
        assert_eq!(fact.error, "");
        drop(guard);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_removes_the_key_when_there_was_none() {
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "[ui]\nprimary = \"zai\"\n";
        std::fs::write(&path, original).unwrap();
        crate::config::set_override_path(&path);
        let facts = default_facts();
        let handle = request_key_check_with(
            &facts,
            &key_check_value("zai", "test-key-value", 4),
            |_| Err("HTTP 429: rate limited".to_string()),
            || {},
        )
        .expect("spawn")
        .unwrap();
        handle.join().unwrap();
        // The refusal removed the key the check wrote; nothing else moved.
        let restored = std::fs::read_to_string(&path).unwrap();
        assert!(!restored.contains("api_key"), "{restored}");
        assert!(restored.contains(r#"primary = "zai""#), "{restored}");
        let fact = facts_snapshot(&facts)
            .key_check
            .expect("the check must publish its verdict");
        assert_eq!(fact.state, KeyCheckState::Failed);
        assert_eq!(fact.error, "HTTP 429: rate limited");
        drop(guard);
    }

    #[test]
    #[cfg(all(unix, any(windows, target_os = "macos")))]
    fn key_check_failed_restore_says_the_key_stayed() {
        use std::os::unix::fs::PermissionsExt;
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[ui]\nprimary = \"zai\"\n").unwrap();
        crate::config::set_override_path(&path);
        let facts = default_facts();
        // The fetch runs inside the thread, between the write and the
        // restore: make the directory read-only there, so the restore's
        // atomic write cannot stage its temp file.
        let locked = dir.path().to_path_buf();
        let lock_target = locked.clone();
        let handle = request_key_check_with(
            &facts,
            &key_check_value("zai", "test-key-value", 5),
            move |_| {
                std::fs::set_permissions(&lock_target, std::fs::Permissions::from_mode(0o555))
                    .unwrap();
                Err("HTTP 401: invalid key".to_string())
            },
            || {},
        )
        .expect("spawn")
        .unwrap();
        handle.join().unwrap();
        // The directory is writable again before the test's temp file is
        // removed.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let fact = facts_snapshot(&facts)
            .key_check
            .expect("the check must publish its verdict");
        assert_eq!(fact.state, KeyCheckState::Failed);
        assert_eq!(
            fact.error,
            "HTTP 401: invalid key The new key stayed in config.toml."
        );
        drop(guard);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_a_newer_send_wins_the_fact() {
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[ui]\nprimary = \"zai\"\n").unwrap();
        crate::config::set_override_path(&path);
        let facts = default_facts();
        // Send 1 blocks inside its fetch while send 2 lands and finishes.
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let handle = request_key_check_with(
            &facts,
            &key_check_value("zai", "first-key", 10),
            move |_| {
                gate_rx.recv().expect("send 2 must release send 1's fetch");
                Ok(r#"{"entries":[{"id":"zai","status":"ready","stale":false}]}"#.into())
            },
            || {},
        )
        .expect("spawn")
        .unwrap();
        // Wait until send 1's `Checking` fact is in place...
        for _ in 0..1000 {
            if facts_snapshot(&facts)
                .key_check
                .is_some_and(|fact| fact.seq == 10 && fact.state == KeyCheckState::Checking)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let fact = facts_snapshot(&facts)
            .key_check
            .expect("send 1 must publish Checking");
        assert_eq!(fact.seq, 10);
        // ...then send 2 replaces the fact and finishes first.
        let handle2 = request_key_check_with(
            &facts,
            &key_check_value("zai", "second-key", 11),
            |_| Err("HTTP 401: invalid key".to_string()),
            || {},
        )
        .expect("spawn")
        .unwrap();
        handle2.join().unwrap();
        // Release send 1: it must not write its verdict over send 2's.
        gate_tx.send(()).unwrap();
        handle.join().unwrap();
        let fact = facts_snapshot(&facts)
            .key_check
            .expect("the verdict must survive");
        assert_eq!(fact.seq, 11, "the newer send's verdict must win");
        assert_eq!(fact.state, KeyCheckState::Failed);
        assert_eq!(fact.error, "HTTP 401: invalid key");
        drop(guard);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn key_check_never_puts_the_key_in_the_fact_or_payload() {
        let guard = override_guard();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[ui]\nprimary = \"zai\"\n").unwrap();
        crate::config::set_override_path(&path);
        let facts = default_facts();
        let handle = request_key_check_with(
            &facts,
            &key_check_value("zai", "test-key-value", 9),
            |_| Err("HTTP 401: rejected".to_string()),
            || {},
        )
        .expect("spawn")
        .unwrap();
        handle.join().unwrap();
        // The key lived in the config while the check ran; it must not leak
        // into the fact, and the serialized payload must not carry it either.
        let snapshot = facts_snapshot(&facts);
        let payload = wrap_report("{}", &snapshot, 0, None);
        let fact = snapshot
            .key_check
            .expect("the check must publish its verdict");
        assert!(!fact.error.contains("test-key-value"), "{}", fact.error);
        assert!(!fact.id.contains("test-key-value"));
        assert_eq!(fact.seq, 9);
        assert!(!payload.to_string().contains("test-key-value"));
        drop(guard);
    }

    #[test]
    fn key_check_result_decides_from_the_report_entry() {
        let ready = r#"{"entries":[{"id":"zai","status":"ready","stale":false}]}"#;
        assert_eq!(
            key_check_result("zai", Ok(ready.to_string())),
            (KeyCheckState::Ok, String::new())
        );
        let error = r#"{"entries":[{"id":"zai","status":"error","stale":false,"error":"HTTP 401: invalid key"}]}"#;
        assert_eq!(
            key_check_result("zai", Ok(error.to_string())),
            (KeyCheckState::Failed, "HTTP 401: invalid key".to_string())
        );
        // A stale reading is a failed check even when the status says ready.
        let stale = r#"{"entries":[{"id":"zai","status":"ready","stale":true,"error":"offline"}]}"#;
        assert_eq!(
            key_check_result("zai", Ok(stale.to_string())),
            (KeyCheckState::Failed, "offline".to_string())
        );
        // No entry for the provider: the fixed note.
        let other = r#"{"entries":[{"id":"openai","status":"ready","stale":false}]}"#;
        assert_eq!(
            key_check_result("zai", Ok(other.to_string())),
            (
                KeyCheckState::Failed,
                "The provider returned no reading.".to_string()
            )
        );
        let broken = key_check_result("zai", Ok("{".to_string()));
        assert_eq!(broken.0, KeyCheckState::Failed);
        assert_eq!(broken.1, "The provider returned no reading.");
        // A fetch error is the error itself.
        assert_eq!(
            key_check_result("zai", Err("no enabled provider matches zai".to_string())),
            (
                KeyCheckState::Failed,
                "no enabled provider matches zai".to_string()
            )
        );
    }

    #[test]
    fn key_check_result_matches_account_entries_and_cuts_the_error_at_300() {
        // An `slug@account` entry still belongs to the provider.
        let account = r#"{"entries":[{"id":"zai@main","status":"ready","stale":false}]}"#;
        assert_eq!(
            key_check_result("zai", Ok(account.to_string())).0,
            KeyCheckState::Ok
        );
        // A slug that is merely a prefix of another id does not match.
        let prefix = r#"{"entries":[{"id":"zai2","status":"ready","stale":false}]}"#;
        assert_eq!(
            key_check_result("zai", Ok(prefix.to_string())).1,
            "The provider returned no reading."
        );
        // The error is cut at 300 characters, on a char boundary...
        let long = "x".repeat(400);
        let report = format!(
            r#"{{"entries":[{{"id":"zai","status":"error","error":"{}"}}]}}"#,
            long
        );
        let (state, error) = key_check_result("zai", Ok(report));
        assert_eq!(state, KeyCheckState::Failed);
        assert_eq!(error.len(), 300);
        // ...which for multi-byte text is a byte count above the char count.
        let wide = "é".repeat(350);
        let report = format!(
            r#"{{"entries":[{{"id":"zai","status":"error","error":"{}"}}]}}"#,
            wide
        );
        let (_, error) = key_check_result("zai", Ok(report));
        assert_eq!(error.chars().count(), 300);
        assert_eq!(error.len(), 600);
    }
}
