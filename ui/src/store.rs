//! Settings and profiles, kept as one JSON file under the app's internal data directory.
//! Profile files live beside it in `ui/profiles/<id>.yaml`, inside the Core's home, since
//! `config.apply` only reads files there.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// `ThemeProps`' colours; None follows the system. Older versions saved `accent` instead.
    pub primary_color: Option<u32>,
    pub primary_colors: Vec<u32>,
    pub color_style: i32,
    #[serde(rename = "accent", skip_serializing)]
    pub legacy_accent: Option<usize>,
    /// 0 follow system, 1 light, 2 dark.
    pub theme_mode: i32,
    /// 0 rule, 1 global, 2 direct.
    pub mode: i32,
    pub proxies: ProxiesStyle,
    /// The dashboard's cards in order; none kept means the defaults.
    pub dashboard: Option<Vec<String>>,
    pub mitm: MitmSettings,
    pub geo_auto_update: bool,
    /// Hours.
    pub geo_update_interval: i64,
    /// `geox-url`, keyed mmdb / asn / geosite.
    pub geox_url: BTreeMap<String, String>,
    /// `AppSettingProps.locale`; None follows the system.
    pub locale: Option<String>,
    pub pure_black: bool,
    pub developer_mode: bool,
    pub auto_check_update: bool,
    pub general: General,
    /// `vpnProps.accessControlProps`.
    pub access_control: crate::access::AccessControl,
    /// `davProps`; None until a server is bound.
    pub dav: Option<crate::backup::Dav>,
    /// `restoreStrategy`: false compatible, true override.
    pub restore_override: bool,
    /// lib/views/config/network.dart on Android.
    pub network: Network,
    /// `PatchClashConfig.dns` with `dnsOverrideKeys`: what the DNS page writes over
    /// every URL and file profile.
    pub dns_override: crate::overrides::OverrideState,
    /// `PatchClashConfig.generalOverrideKeys` with their values.
    pub general_override: crate::overrides::OverrideState,
    /// `tunOverride` with `tunOverrideKeys`, written over every profile's `tun`.
    pub tun_override: crate::overrides::OverrideState,
    /// The sniffer page's keys, written over every profile's `sniffer`.
    pub sniffer_override: crate::overrides::OverrideState,
    /// The NTP page's keys, written over every profile's `ntp`.
    pub ntp_override: crate::overrides::OverrideState,
    /// The added rules (`globalRulesProvider`), put before every profile's rules.
    pub added_rules: Vec<crate::rules::Rule>,
    /// `AppSettingProps.filters`.
    pub filters: Vec<crate::items::Filter>,
    /// The scripts (`scriptsProvider`); each one's content is `scripts/<id>.js`.
    pub scripts: Vec<crate::items::Script>,
    /// `AppSettingProps.hideIp`: the outbound IP shown masked.
    pub hide_ip: bool,
    /// `AppSettingProps.serviceOrder`: service ids the user moved, the rest after them.
    pub service_order: Vec<String>,
    /// `AppSettingProps.disabledServices`.
    pub disabled_services: Vec<String>,
    /// `AppSettingProps.tableColumnWidths`: dragged column widths by table id.
    pub table_column_widths: BTreeMap<String, BTreeMap<String, f32>>,
    /// `Config.excludeSSIDs`: Wi-Fi networks a run is suspended on.
    pub exclude_ssids: Vec<String>,
    /// `AppSettingProps.floatingNavigationBar`: the phone's dock floats, else a docked bar.
    pub floating_navigation_bar: bool,
    /// `AppSettingProps.tabAnimation`: 0 slide, 1 fade.
    pub tab_animation: i32,
    pub font_family: Option<String>,
    pub text_scale: TextScale,
    /// `ClashProvider`s, in the page's order.
    pub rule_sets: Vec<crate::rule_sets::RuleSet>,
    /// `IconSet`s, in the page's order.
    pub icon_sets: Vec<crate::icon_sets::IconSet>,
    /// `CustomProxy`s, in the page's order.
    pub custom_proxies: Vec<crate::custom_proxies::CustomProxy>,
    pub mitm_modules: Vec<crate::mitm::MitmModule>,
}

/// `TextScale`: off follows the system's font scale.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TextScale {
    pub enable: bool,
    pub scale: f32,
}

impl Default for TextScale {
    fn default() -> Self {
        TextScale {
            enable: false,
            scale: 1.0,
        }
    }
}

/// The network page's settings: `VpnProps`, `NetworkProps` and the TUN part of
/// `PatchClashConfig` that Android uses.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Network {
    pub vpn: bool,
    pub system_proxy: bool,
    pub bypass_domain: Vec<String>,
    pub allow_bypass: bool,
    pub ipv6: bool,
    pub dns_hijacking: bool,
    pub mtu: i64,
    pub congestion_controller: String,
    pub bypass_private_route: bool,
    pub route_address: Vec<String>,
    pub route_exclude_address: Vec<String>,
}

/// `minTunMtu`, `maxTunMtu`, `defaultTunMtu` and `congestionControllers` in
/// lib/models/clash_config.dart.
pub const TUN_MTU: (i64, i64, i64) = (1280, 65535, 9000);
pub const CONGESTION_CONTROLLERS: [&str; 3] = ["cubic", "reno", "bbr"];

impl Default for Network {
    fn default() -> Self {
        Network {
            vpn: true,
            system_proxy: true,
            bypass_domain: crate::profile::BYPASS_DOMAIN
                .iter()
                .map(|d| d.to_string())
                .collect(),
            allow_bypass: true,
            ipv6: false,
            dns_hijacking: false,
            mtu: TUN_MTU.2,
            congestion_controller: CONGESTION_CONTROLLERS[0].into(),
            bypass_private_route: true,
            route_address: Vec::new(),
            route_exclude_address: Vec::new(),
        }
    }
}

/// What lib/views/config/general.dart sets: `AppSettingProps`, `PatchClashConfig` and
/// `NetworkProps` fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct General {
    pub auto_run: bool,
    pub minimize_on_exit: bool,
    /// `hidden`: kept out of the recent tasks.
    pub hidden: bool,
    pub show_notification_stop_action: bool,
    /// `globalUa`; None sends the default.
    pub global_ua: Option<String>,
    /// The user agents offered beside the default (`userAgents`).
    pub user_agents: Vec<String>,
    pub check_certificate: bool,
    pub mixed_port: i64,
    /// `port` and `socksPort`; 0 is off.
    pub http_port: i64,
    pub socks_port: i64,
    pub allow_lan: bool,
    pub external_controller: bool,
    pub secret: String,
    pub external_controller_cors: bool,
    pub authentication: Authentication,
    /// Milliseconds.
    pub delay_test_timeout: i64,
    pub find_process_always: bool,
    pub close_connections: bool,
    pub only_statistics_proxy: bool,
    pub hosts: BTreeMap<String, String>,
    pub append_system_dns: bool,
    /// `openLogs`: the logs page and its entry.
    pub open_logs: bool,
    /// `logLevel`, as `LogLevel.name`.
    pub log_level: String,
}

impl Default for General {
    fn default() -> Self {
        General {
            auto_run: false,
            minimize_on_exit: true,
            hidden: false,
            show_notification_stop_action: true,
            global_ua: None,
            user_agents: DEFAULT_USER_AGENTS.iter().map(|s| s.to_string()).collect(),
            check_certificate: true,
            mixed_port: 7890,
            http_port: 0,
            socks_port: 0,
            allow_lan: false,
            external_controller: false,
            secret: String::new(),
            external_controller_cors: false,
            authentication: Authentication::default(),
            delay_test_timeout: DEFAULT_DELAY_TEST_TIMEOUT,
            find_process_always: false,
            close_connections: true,
            only_statistics_proxy: false,
            hosts: BTreeMap::new(),
            append_system_dns: false,
            open_logs: false,
            log_level: "error".into(),
        }
    }
}

/// `AuthenticationProps`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Authentication {
    pub enable: bool,
    pub username: String,
    pub password: String,
}

/// `defaultUserAgents` in lib/models/config.dart.
pub const DEFAULT_USER_AGENTS: [&str; 2] = ["clash-verge/v2.4.2", "ClashforWindows/0.19.23"];
/// `delayTestTimeoutOptions` and `defaultDelayTestTimeout` in lib/common/constant.dart.
pub const DELAY_TEST_TIMEOUTS: [i64; 5] = [500, 1000, 2000, 5000, 8000];
pub const DEFAULT_DELAY_TEST_TIMEOUT: i64 = 8000;

impl Settings {
    pub fn migrate(&mut self) {
        if let Some(i) = self.legacy_accent.take() {
            let accents = crate::theme::ACCENTS;
            self.primary_color = Some(accents[i.min(accents.len() - 1)]);
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            primary_color: Some(crate::colors::DEFAULT_PRIMARY),
            primary_colors: crate::theme::ACCENTS.to_vec(),
            color_style: crate::colors::VIVID,
            legacy_accent: None,
            theme_mode: 0,
            mode: 0,
            proxies: ProxiesStyle::default(),
            dashboard: None,
            mitm: MitmSettings::default(),
            geo_auto_update: true,
            geo_update_interval: 24,
            geox_url: crate::resources::default_urls(),
            locale: None,
            pure_black: false,
            developer_mode: false,
            auto_check_update: true,
            general: General::default(),
            access_control: Default::default(),
            dav: None,
            restore_override: false,
            network: Network::default(),
            dns_override: Default::default(),
            general_override: Default::default(),
            tun_override: Default::default(),
            sniffer_override: Default::default(),
            ntp_override: Default::default(),
            added_rules: Vec::new(),
            filters: Vec::new(),
            scripts: Vec::new(),
            hide_ip: false,
            service_order: Vec::new(),
            disabled_services: Vec::new(),
            table_column_widths: BTreeMap::new(),
            exclude_ssids: Vec::new(),
            floating_navigation_bar: true,
            tab_animation: 1,
            font_family: None,
            text_scale: TextScale::default(),
            rule_sets: Vec::new(),
            icon_sets: Vec::new(),
            custom_proxies: Vec::new(),
            mitm_modules: Vec::new(),
        }
    }
}

/// `Mitm` in lib/models/clash_config.dart.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MitmSettings {
    pub enable: bool,
    pub hostname: Vec<String>,
    pub capture: bool,
    /// None keeps the platform's limit.
    pub capture_entries: Option<i64>,
    pub skip_server_cert_verify: bool,
    pub auto_quic_block: bool,
    pub h2: bool,
    pub decrypt_untrusted: bool,
    pub trusted_fingerprint: Option<String>,
}

impl Default for MitmSettings {
    fn default() -> Self {
        MitmSettings {
            enable: false,
            hostname: Vec::new(),
            capture: false,
            capture_entries: None,
            skip_server_cert_verify: false,
            auto_quic_block: true,
            h2: false,
            decrypt_untrusted: false,
            trusted_fingerprint: None,
        }
    }
}

/// `ProxiesStyleProps` in lib/models/config.dart.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProxiesStyle {
    /// 0 tab, 1 list.
    pub kind: i32,
    /// 0 default, 1 delay, 2 name.
    pub sort: i32,
    /// 0 tight, 1 standard, 2 loose.
    pub layout: i32,
    /// 0 compact, 1 standard, 2 loose.
    pub card: i32,
    /// 0 filled, 1 plain, 2 hidden.
    pub icon: i32,
    pub hide_timeout: bool,
    /// The groups unfolded in the list style (`unfoldSet`).
    pub unfold: Vec<String>,
}

impl Default for ProxiesStyle {
    fn default() -> Self {
        ProxiesStyle {
            kind: 1,
            sort: 0,
            layout: 1,
            card: 1,
            icon: 0,
            hide_timeout: false,
            unfold: Vec::new(),
        }
    }
}

/// `SubscriptionInfo` in lib/models/profile.dart, from the `subscription-userinfo` header.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Subscription {
    pub upload: i64,
    pub download: i64,
    pub total: i64,
    /// Unix seconds, 0 for none.
    pub expire: i64,
}

impl Subscription {
    /// `SubscriptionInfo.formHString`.
    pub fn parse(header: &str) -> Subscription {
        let mut sub = Subscription::default();
        for field in header.split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let Some(n) = value.parse::<i64>().ok().or_else(|| {
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite())
                    .map(|f| f.trunc() as i64)
            }) else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "upload" => sub.upload = n,
                "download" => sub.download = n,
                "total" => sub.total = n,
                "expire" => sub.expire = n,
                _ => {}
            }
        }
        sub
    }
}

/// `defaultUpdateDuration` in lib/common/constant.dart: one day.
pub const DEFAULT_UPDATE_MINUTES: u64 = 24 * 60;

fn yes() -> bool {
    true
}

fn default_interval() -> u64 {
    DEFAULT_UPDATE_MINUTES
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    /// The label as the user or the download named it; empty falls back to `default_label`.
    pub name: String,
    pub url: String,
    /// "url", "file" or "custom" (`ProfileType`); empty in files written before it existed,
    /// read as "url" when there is a url.
    pub kind: String,
    /// Unix seconds of the last successful download or save.
    pub updated: u64,
    #[serde(default = "yes")]
    pub auto_update: bool,
    #[serde(default = "default_interval")]
    pub interval_minutes: u64,
    pub age_key: String,
    pub subscription: Option<Subscription>,
    /// The proxy picked in each group, handed to `config.apply` as `selection`.
    pub selection: BTreeMap<String, String>,
    pub overrides: crate::custom_profile::ProfileOverrides,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            id: String::new(),
            name: String::new(),
            url: String::new(),
            kind: String::new(),
            updated: 0,
            auto_update: true,
            interval_minutes: DEFAULT_UPDATE_MINUTES,
            age_key: String::new(),
            subscription: None,
            selection: BTreeMap::new(),
            overrides: Default::default(),
        }
    }
}

impl Profile {
    pub fn is_url(&self) -> bool {
        self.kind == "url" || (self.kind.is_empty() && !self.url.is_empty())
    }

    pub fn is_custom(&self) -> bool {
        self.kind == crate::custom_profile::KIND
    }

    /// `realLabel`: the label, else the url's host, else the id.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        host_of(&self.url).unwrap_or_else(|| self.id.clone())
    }

    /// `exportFileName`.
    pub fn export_name(&self) -> String {
        let label = self.label();
        let lower = label.to_lowercase();
        if lower.ends_with(".yaml") || lower.ends_with(".yml") {
            label
        } else {
            format!("{label}.yaml")
        }
    }

    /// `realAutoUpdate` and the check in `autoUpdateProfiles`.
    pub fn due(&self, now: u64) -> bool {
        self.is_url() && self.auto_update && self.updated + self.interval_minutes.max(1) * 60 <= now
    }
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]"))?
    } else {
        host.split(':').next()?.to_string()
    };
    (!host.is_empty()).then_some(host)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub active: Option<String>,
    pub proxy_groups: Vec<crate::custom_profile::ProxyGroupRow>,
    pub profile_rules: Vec<crate::custom_profile::ProfileRule>,
    pub proxy_dialers: Vec<crate::custom_profile::ProxyDialer>,
    pub disabled_rules: Vec<crate::custom_profile::DisabledRule>,
}

pub struct Store {
    pub dir: PathBuf,
    pub state: State,
}

pub fn mode_name(mode: i32) -> &'static str {
    match mode {
        1 => "global",
        2 => "direct",
        _ => "rule",
    }
}

impl Store {
    pub fn open(home: &Path) -> Store {
        let dir = home.join("ui");
        let _ = std::fs::create_dir_all(dir.join("profiles"));
        let mut state: State = std::fs::read(dir.join("state.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        state.settings.migrate();
        Store { dir, state }
    }

    pub fn save(&self) {
        let Ok(bytes) = serde_json::to_vec_pretty(&self.state) else {
            return;
        };
        let tmp = self.dir.join("state.json.part");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, self.dir.join("state.json"));
        }
    }

    pub fn profile_path(&self, id: &str) -> PathBuf {
        self.dir.join("profiles").join(format!("{id}.yaml"))
    }

    pub fn temp_path(&self) -> PathBuf {
        self.dir.join("download.part.yaml")
    }

    pub fn active(&self) -> Option<&Profile> {
        let id = self.state.active.as_ref()?;
        self.state.profiles.iter().find(|p| &p.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.state.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn remove(&mut self, id: &str) {
        self.state.profiles.retain(|p| p.id != id);
        crate::custom_profile::remove_profile(&mut self.state, id);
        let _ = std::fs::remove_file(self.profile_path(id));
        if self.state.active.as_deref() == Some(id) {
            self.state.active = self.state.profiles.first().map(|p| p.id.clone());
        }
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_survives_a_round_trip() {
        let home = std::env::temp_dir().join(format!("sc-store-{}", std::process::id()));
        let mut store = Store::open(&home);
        store.state.settings.primary_color = None;
        store.state.settings.color_style = 2;
        store.state.profiles.push(Profile {
            id: "1".into(),
            name: "a".into(),
            ..Default::default()
        });
        store.state.active = Some("1".into());
        store.save();
        let again = Store::open(&home);
        assert!(again.state.profiles[0].auto_update);
        assert!(again.state.settings.geo_auto_update);
        assert_eq!(again.state.settings.geo_update_interval, 24);
        assert_eq!(again.state, store.state);
        assert_eq!(again.active().unwrap().name, "a");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn older_accent_index_becomes_the_primary_colour() {
        let mut s: Settings = serde_json::from_str(r#"{"accent":2}"#).unwrap();
        s.migrate();
        assert_eq!(s.primary_color, Some(crate::theme::ACCENTS[2]));
        assert!(!serde_json::to_string(&s).unwrap().contains("accent"));
        let mut s: Settings = serde_json::from_str(r#"{"primary_color":null}"#).unwrap();
        s.migrate();
        assert_eq!(s.primary_color, None);
        assert_eq!(s.primary_colors, crate::theme::ACCENTS);
    }

    #[test]
    fn older_settings_take_the_geo_defaults() {
        let s: Settings = serde_json::from_str(r#"{"accent":2}"#).unwrap();
        assert!(s.geo_auto_update && s.geox_url.len() == 3 && !s.mitm.capture);
    }

    #[test]
    fn older_settings_float_the_dock_and_fade_tabs() {
        let s: Settings = serde_json::from_str(r#"{"accent":2}"#).unwrap();
        assert!(s.floating_navigation_bar);
        assert_eq!(s.tab_animation, 1);
        assert_eq!(s.font_family, None);
        assert_eq!(s.text_scale, TextScale::default());
    }

    #[test]
    fn older_settings_take_flclash_general_defaults() {
        let s: Settings = serde_json::from_str(r#"{"accent":2}"#).unwrap();
        let g = &s.general;
        assert!(s.locale.is_none() && !s.developer_mode && s.auto_check_update);
        assert!(g.check_certificate && g.close_connections && g.show_notification_stop_action);
        assert!(!g.open_logs && !g.allow_lan && !g.authentication.enable);
        assert_eq!((g.mixed_port, g.delay_test_timeout), (7890, 8000));
        assert_eq!(g.user_agents.len(), 2);
    }

    #[test]
    fn subscription_header_reads_like_flclash() {
        let s = Subscription::parse("upload=10; download=20.7;total=100; expire=1700000000; x=y");
        assert_eq!(
            s,
            Subscription {
                upload: 10,
                download: 20,
                total: 100,
                expire: 1700000000
            }
        );
    }

    #[test]
    fn older_files_read_as_url_profiles_with_auto_update() {
        let p: Profile =
            serde_json::from_str(r#"{"id":"1","url":"https://a.example.com:8443/x"}"#).unwrap();
        assert!(p.is_url() && p.auto_update);
        assert_eq!(p.interval_minutes, DEFAULT_UPDATE_MINUTES);
        assert_eq!(p.label(), "a.example.com");
        assert_eq!(p.export_name(), "a.example.com.yaml");
    }

    #[test]
    fn removing_the_active_profile_moves_on() {
        let home = std::env::temp_dir().join(format!("sc-store-rm-{}", std::process::id()));
        let mut store = Store::open(&home);
        for id in ["1", "2"] {
            store.state.profiles.push(Profile {
                id: id.into(),
                ..Default::default()
            });
        }
        store.state.active = Some("1".into());
        store.remove("1");
        assert_eq!(store.state.active.as_deref(), Some("2"));
        let _ = std::fs::remove_dir_all(home);
    }
}
