//! Settings → Providers: every built-in provider from [`crate::catalog`], the
//! config switch that turns one on or off, and the action that signs it in.
//!
//! The report only carries enabled providers, so a popover that lists its
//! cards cannot show — or enable — the ones that are off. This is the row
//! source for those, kept in Rust so the popover holds no provider table.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::payload::{HostFacts, SharedFacts, with_facts};
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
                Some(json!({
                    "id": row.id,
                    "name": row.name,
                    "kind": row.kind.as_str(),
                    "enabled": row.enabled,
                    "configured": row.configured,
                    "env": row.env,
                    "sign_in": vendor.sign_in_hint(),
                    "action": sign_in_for(vendor, macos).kind(),
                    "add_accounts": add_account_kinds(vendor, macos),
                }))
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

/// A catalog read, and the switch generation it started under.
struct CatalogRead {
    rows: Vec<VendorStatus>,
    generation: u64,
}

/// Re-read the catalog into `facts` when one was requested (and at startup);
/// `true` when the rows changed hands. An unreadable config lists every
/// provider against the defaults. The read runs outside the lock, so a
/// switch can land while it does; see [`publish`].
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn refresh_due(facts: &SharedFacts) -> bool {
    if !DUE.swap(false, Ordering::Relaxed) {
        return false;
    }
    let generation = facts.lock().map(|f| f.providers_generation).unwrap_or(0);
    let rows = crate::catalog::statuses(&crate::config::Config::load().unwrap_or_default());
    publish(facts, CatalogRead { rows, generation })
}

/// Hand a read to the popover unless a switch moved since it started: that
/// read may hold the config from before the write, and publishing it would
/// put the switch back. The switch requested another read, which wins.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn publish(facts: &SharedFacts, read: CatalogRead) -> bool {
    let mut published = false;
    with_facts(facts, |f| {
        if f.providers_generation == read.generation {
            f.providers = read.rows;
            published = true;
        }
    });
    published
}

/// `{"cmd":"set-provider-enabled","id":"cursor","value":true}`: flip the
/// provider's `enabled` in config.toml, then in the catalog `facts` holds.
///
/// The popover's switch shows the host's row, not the click, and a re-read
/// catalog only reaches it with the next report — after every enabled
/// provider has been fetched, which takes seconds when one starts `agy` or
/// waits on the network. Flipping the row here moves the switch on the next
/// push; the re-read still follows, for `configured` and the sign-in action.
/// It is requested after the flip, so it starts under the new generation.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn set_enabled(value: &Value, facts: &SharedFacts) {
    let written = write_enabled(value);
    with_facts(facts, |f| record_enabled(f, written));
    request();
}

/// The slug is validated by [`crate::config::set_vendor_enabled`], so no
/// other section is reachable.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn write_enabled(value: &Value) -> Result<(&str, bool), String> {
    let (Some(id), Some(enabled)) = (
        value.get("id").and_then(Value::as_str),
        value.get("value").and_then(Value::as_bool),
    ) else {
        return Err("Malformed provider request.".into());
    };
    let path = crate::config::resolved_path()
        .ok_or_else(|| "Could not resolve the config.toml path.".to_string())?;
    crate::config::set_vendor_enabled(&path, id, enabled)
        .map(|()| (id, enabled))
        .map_err(|error| error.user_message())
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn record_enabled(facts: &mut HostFacts, written: Result<(&str, bool), String>) {
    facts.providers_error = match written {
        Ok((id, enabled)) => {
            if let Some(row) = facts.providers.iter_mut().find(|row| row.id == id) {
                row.enabled = enabled;
            }
            facts.providers_generation = facts.providers_generation.wrapping_add(1);
            String::new()
        }
        Err(error) => error,
    };
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

    fn default_rows() -> Vec<VendorStatus> {
        crate::catalog::statuses_with(
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
        )
    }

    #[test]
    fn rows_carry_every_vendor_with_hint_and_action() {
        let rows = default_rows();
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
        assert!(write_enabled(&json!({"id": "cursor"})).is_err());
        assert!(write_enabled(&json!({"value": true})).is_err());
    }

    /// A catalog row built by hand, so these tests resolve no config default.
    fn row(id: &'static str, enabled: bool) -> VendorStatus {
        VendorStatus {
            id,
            name: id,
            short_name: id,
            kind: AuthKind::Oauth,
            enabled,
            configured: true,
            needs_credential: true,
            env: String::new(),
            login: "",
        }
    }

    fn enabled(facts: &HostFacts, id: &str) -> bool {
        facts
            .providers
            .iter()
            .find(|row| row.id == id)
            .unwrap()
            .enabled
    }

    #[test]
    fn a_written_switch_moves_its_row_before_any_report() {
        let mut facts = HostFacts::new("0.0.0", false);
        facts.providers = vec![row("cursor", false)];
        facts.providers_error = "stale".into();

        record_enabled(&mut facts, Ok(("cursor", true)));
        assert!(enabled(&facts, "cursor"));
        assert_eq!(facts.providers_error, "");
        assert_eq!(facts.providers_generation, 1);

        record_enabled(&mut facts, Err("Config is read-only.".into()));
        assert!(enabled(&facts, "cursor"), "a failed write leaves the row");
        assert_eq!(facts.providers_error, "Config is read-only.");
        assert_eq!(facts.providers_generation, 1, "nothing moved");
    }

    #[test]
    fn a_catalog_read_from_before_a_switch_cannot_put_it_back() {
        let facts: SharedFacts =
            std::sync::Arc::new(std::sync::Mutex::new(HostFacts::new("0.0.0", false)));
        with_facts(&facts, |f| f.providers = vec![row("cursor", false)]);
        // The worker starts reading while Cursor is still off…
        let stale = CatalogRead {
            rows: vec![row("cursor", false)],
            generation: facts.lock().unwrap().providers_generation,
        };
        // …the switch lands before the read is published…
        with_facts(&facts, |f| record_enabled(f, Ok(("cursor", true))));
        // …and the old rows are dropped instead of turning it back off.
        assert!(!publish(&facts, stale));
        assert!(enabled(&facts.lock().unwrap(), "cursor"));

        let fresh = CatalogRead {
            rows: vec![row("cursor", true)],
            generation: facts.lock().unwrap().providers_generation,
        };
        assert!(publish(&facts, fresh));
        assert!(enabled(&facts.lock().unwrap(), "cursor"));
    }
}
