//! Which proxy Banager's own requests go through (U12 of the decisions
//! round): the one the proxy settings its commands get name
//! (`runner::login_path::command_var`), so a check Banager makes itself
//! goes the way a `brew` it runs does; failing that, the one this Mac's
//! own network settings name (`system_proxy`), as reqwest chose before --
//! except to this Mac itself, which is never sent through a proxy: a
//! proxy on another machine cannot reach the Ollama on this one, and one
//! on this Mac has no reason to be asked.
//!
//! The rules are like curl's, which Homebrew downloads with: `https_proxy`
//! for an https address and `http_proxy` for an http one, each read in
//! lowercase first and then in uppercase, then `all_proxy` / `ALL_PROXY`;
//! a setting with nothing in it counts as not set; and `no_proxy` /
//! `NO_PROXY`, a comma-separated list of names, addresses and address
//! ranges, sends what it names straight. Two things differ from curl's:
//! `HTTP_PROXY` in uppercase is read for an http address too, as Go
//! programs such as `ollama` read it (curl reads only the lowercase one
//! there, against "httpoxy", which a desktop app does not meet); and a
//! `no_proxy` entry starting `*.` names what the rest of it names (curl
//! takes no wildcard but a lone `*`). This decides only the way, never
//! the destination: the hosts a request may go to are still the ones
//! `real::host_allowed` lets through.

use std::net::{IpAddr, Ipv4Addr};
use url::{Host, Url};

/// The proxy Banager's own request to `url` goes through, as the setting
/// names it, with `var` giving each setting's value; when none names one
/// for `url`'s scheme, the one `system` gives, which is asked only then
/// (`system_proxy`); `None` to connect straight -- always so for this Mac
/// itself (`is_this_mac`) and for what `no_proxy` names, without asking
/// `system`.
pub fn proxy_for(
    url: &Url,
    var: impl Fn(&str) -> Option<String>,
    system: impl FnOnce(&Url) -> Option<String>,
) -> Option<String> {
    let host = url.host()?;
    if is_this_mac(&host) {
        return None;
    }
    let first = |names: &[&str]| {
        names.iter().find_map(|name| {
            var(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
    };
    if first(&["no_proxy", "NO_PROXY"]).is_some_and(|list| listed(&host, &list)) {
        return None;
    }
    let named = match url.scheme() {
        "https" => first(&["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]),
        "http" => first(&["http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"]),
        _ => return None,
    };
    named.or_else(|| system(url))
}

/// Every setting `proxy_for` reads through its `var`: each one of
/// `runner::login_path::IMPORTED`, so `command_var` answers for it.
pub const SETTINGS: &[&str] = &[
    "http_proxy",
    "HTTP_PROXY",
    "https_proxy",
    "HTTPS_PROXY",
    "all_proxy",
    "ALL_PROXY",
    "no_proxy",
    "NO_PROXY",
];

/// The value each of `SETTINGS` had at one moment, as a `var` gave it:
/// what one `RealHttpClient` connection pool is kept for (R5 of the f19
/// review). Two are equal when every setting is. No `Debug`: a proxy's
/// value can hold a password.
#[derive(Clone, PartialEq, Eq)]
pub struct Settings(Vec<Option<String>>);

impl Settings {
    /// Each of `SETTINGS` as `var` gives it now.
    pub fn read(var: impl Fn(&str) -> Option<String>) -> Settings {
        Settings(SETTINGS.iter().map(|name| var(name)).collect())
    }

    /// `name`'s value as it was read, for `proxy_for`'s `var`; `None` for
    /// a name that is not one of `SETTINGS`.
    pub fn var(&self, name: &str) -> Option<String> {
        SETTINGS
            .iter()
            .position(|setting| *setting == name)
            .and_then(|at| self.0[at].clone())
    }
}

/// The proxy this Mac's own network settings name for `url` (System
/// Settings > Network > Details > Proxies): its web proxy (HTTP) for an
/// http address, its secure web proxy (HTTPS) for an https one -- what a
/// proxy app such as Clash Verge, ClashX or Surge sets in its "system
/// proxy" mode, with nothing exported in a shell. reqwest read these for
/// Banager until it was given a proxy of its own (`ClientBuilder::proxy`
/// turns its own reading off), with hyper-util's `Matcher::from_system`,
/// which is read here too when selecting a proxy for a new connection.
/// That reads the process environment first, which `proxy_for` has
/// already found nothing in when it asks. Existing pooled connections can
/// keep their previous route after settings change. Neither the SOCKS
/// proxy nor the list of hosts the settings bypass is read, as reqwest
/// did not read them either.
pub fn system_proxy(url: &Url) -> Option<String> {
    from_matcher(
        &hyper_util::client::proxy::matcher::Matcher::from_system(),
        url,
    )
}

/// The proxy `matcher` names for `url`, as an address reqwest reads:
/// `http://host:port/` for the `host:port` macOS keeps.
fn from_matcher(
    matcher: &hyper_util::client::proxy::matcher::Matcher,
    url: &Url,
) -> Option<String> {
    let uri: http::Uri = url.as_str().parse().ok()?;
    matcher
        .intercept(&uri)
        .map(|intercept| intercept.uri().to_string())
}

/// Whether `host` is this Mac: `localhost` and any name under it
/// (RFC 6761), a loopback address (`127.0.0.0/8`, `::1`, and IPv4's
/// written as IPv6), or the unspecified address (`0.0.0.0`, `::`), which
/// reaches this Mac too -- an `OLLAMA_HOST` of `0.0.0.0` is common.
pub fn is_this_mac(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(name) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name == "localhost" || name.ends_with(".localhost")
        }
        Host::Ipv4(ip) => ipv4_is_this_mac(*ip),
        Host::Ipv6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.to_ipv4_mapped().is_some_and(ipv4_is_this_mac)
        }
    }
}

fn ipv4_is_this_mac(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_unspecified()
}

/// Whether `no_proxy`, read as curl reads it, names `host`: `*` names
/// every host; a name names itself and every name under it, with or
/// without a leading `.` -- or `*.`, which curl does not take; an address
/// names itself, and an address with a `/` and a prefix length the range
/// it starts.
fn listed(host: &Host<&str>, no_proxy: &str) -> bool {
    no_proxy
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry == "*" {
                return true;
            }
            let bare = entry.trim_start_matches('[').trim_end_matches(']');
            if let Some(range) = Range::parse(bare) {
                return match host {
                    Host::Ipv4(ip) => range.contains(IpAddr::V4(*ip)),
                    Host::Ipv6(ip) => range.contains(IpAddr::V6(*ip)),
                    Host::Domain(_) => false,
                };
            }
            let Host::Domain(name) = host else {
                return false;
            };
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            let entry = entry
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .trim_end_matches('.')
                .to_ascii_lowercase();
            !entry.is_empty() && (name == entry || name.ends_with(&format!(".{entry}")))
        })
}

/// An address, or a range of them: `192.168.0.0/16`, `fd00::/8`.
struct Range {
    start: IpAddr,
    prefix: u32,
}

impl Range {
    fn parse(entry: &str) -> Option<Range> {
        let (address, prefix) = match entry.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix.parse::<u32>().ok()?)),
            None => (entry, None),
        };
        let start: IpAddr = address.parse().ok()?;
        let bits = if start.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(bits);
        (prefix <= bits).then_some(Range { start, prefix })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.start, ip) {
            (IpAddr::V4(start), IpAddr::V4(ip)) => {
                let mask = u32::MAX.checked_shl(32 - self.prefix).unwrap_or(0);
                u32::from(start) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(start), IpAddr::V6(ip)) => {
                let mask = u128::MAX.checked_shl(128 - self.prefix).unwrap_or(0);
                u128::from(start) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lookup over `set`, as `command_var` answers.
    fn vars(set: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let set: Vec<(String, String)> = set
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        move |name| {
            set.iter()
                .find(|(set, _)| set == name)
                .map(|(_, value)| value.clone())
        }
    }

    /// With no proxy in this Mac's network settings.
    fn through(url: &str, set: &[(&str, &str)]) -> Option<String> {
        proxy_for(&Url::parse(url).unwrap(), vars(set), |_| None)
    }

    /// With `system` as the proxy this Mac's network settings name, and
    /// whether it was asked.
    fn through_with_system(url: &str, set: &[(&str, &str)]) -> (Option<String>, bool) {
        let asked = std::cell::Cell::new(false);
        let found = proxy_for(&Url::parse(url).unwrap(), vars(set), |asked_for| {
            asked.set(true);
            assert_eq!(asked_for.as_str(), url, "asked for another address");
            Some(format!("http://system.proxy:{}", asked_for.scheme().len()))
        });
        (found, asked.get())
    }

    /// The finding of the U12 review: a proxy app in its "system proxy"
    /// mode (Clash Verge, ClashX, Surge) sets only this Mac's network
    /// settings, which reqwest read for Banager before it named its own
    /// proxy. They are asked last -- after the login shell's setting and
    /// the environment's (`vars`, as `command_var` answers) -- and never
    /// for this Mac itself or what `no_proxy` names.
    #[test]
    fn test_this_macs_network_settings_are_asked_last_and_never_for_this_mac_or_no_proxy() {
        let crates = "https://crates.io/api/v1/crates/ripgrep";
        let remote = "http://192.168.1.20:11434/api/tags";
        // Nothing set: this Mac's network settings, for either scheme.
        assert_eq!(
            through_with_system(crates, &[]),
            (Some("http://system.proxy:5".into()), true)
        );
        assert_eq!(
            through_with_system(remote, &[]),
            (Some("http://system.proxy:4".into()), true)
        );
        // A setting for the scheme comes first, and the network settings
        // are not asked.
        for set in [
            &[("https_proxy", "http://a:1")][..],
            &[("HTTPS_PROXY", "http://a:1")][..],
            &[("all_proxy", "http://a:1")][..],
        ] {
            assert_eq!(
                through_with_system(crates, set),
                (Some("http://a:1".into()), false),
                "{set:?}"
            );
        }
        // A setting for the other scheme, or an empty one, is not one for
        // this: the network settings are asked.
        assert_eq!(
            through_with_system(
                crates,
                &[("http_proxy", "http://a:1"), ("https_proxy", " ")]
            ),
            (Some("http://system.proxy:5".into()), true)
        );
        // This Mac, and what `no_proxy` names: straight, without asking.
        for url in [
            "http://127.0.0.1:11434/api/tags",
            "http://localhost:11434/api/tags",
            "http://[::1]:11434/",
            "http://0.0.0.0:11434/",
        ] {
            assert_eq!(through_with_system(url, &[]), (None, false), "{url}");
        }
        assert_eq!(
            through_with_system(crates, &[("no_proxy", "crates.io")]),
            (None, false)
        );
        assert_eq!(
            through_with_system(remote, &[("NO_PROXY", "192.168.0.0/16")]),
            (None, false)
        );
    }

    /// What this Mac's network settings name is handed to reqwest as a
    /// proxy address it reads: `host:port`, as macOS keeps them, becomes
    /// an http proxy, each for its own scheme.
    #[test]
    fn test_a_proxy_named_in_this_macs_network_settings_becomes_an_http_proxy_address() {
        use hyper_util::client::proxy::matcher::Matcher;
        let settings = Matcher::builder()
            .http("127.0.0.1:7897")
            .https("proxy.lan:8443")
            .build();
        let named = |url: &str| from_matcher(&settings, &Url::parse(url).unwrap());
        assert_eq!(
            named("https://crates.io/api/v1/crates/ripgrep"),
            Some("http://proxy.lan:8443/".into())
        );
        assert_eq!(
            named("http://192.168.1.20:11434/api/tags"),
            Some("http://127.0.0.1:7897/".into())
        );
        let proxy = reqwest::Proxy::all(named("https://pypi.org/pypi/httpie/json").unwrap());
        assert!(proxy.is_ok(), "{proxy:?}");
        // Neither set: none.
        let unset = Matcher::builder().build();
        assert_eq!(
            from_matcher(&unset, &Url::parse("https://pypi.org/").unwrap()),
            None
        );
    }

    /// R5 of the f19 review: a `RealHttpClient` pool is kept for one
    /// `Settings`, and its requests choose their proxy from that alone --
    /// so `Settings` must hold every setting `proxy_for` reads, or a change
    /// to one it left out would reach a pool kept for other settings. And
    /// each must be one `command_var` answers for.
    #[test]
    fn test_settings_hold_every_setting_proxy_for_reads() {
        let asked = std::cell::RefCell::new(Vec::<String>::new());
        let record = |name: &str| {
            asked.borrow_mut().push(name.to_string());
            None
        };
        for url in [
            "https://crates.io/api/v1/crates/ripgrep",
            "http://192.168.1.20:11434/api/tags",
        ] {
            proxy_for(&Url::parse(url).unwrap(), record, |_| None);
        }
        let asked = asked.into_inner();
        for name in &asked {
            assert!(SETTINGS.contains(&name.as_str()), "{name} is not kept");
        }
        for name in SETTINGS {
            assert!(asked.iter().any(|asked| asked == name), "{name} is unread");
            assert!(
                crate::runner::login_path::IMPORTED.contains(name),
                "{name} is not imported"
            );
        }
        // What was read is what `var` gives, and a change to any one
        // setting makes other settings.
        let read = Settings::read(vars(&[
            ("http_proxy", "http://someone:secret@a:1"),
            ("NO_PROXY", "straight.invalid"),
        ]));
        assert_eq!(
            read.var("http_proxy").as_deref(),
            Some("http://someone:secret@a:1")
        );
        assert_eq!(read.var("NO_PROXY").as_deref(), Some("straight.invalid"));
        assert_eq!(read.var("https_proxy"), None);
        assert_eq!(read.var("PIP_INDEX_URL"), None);
        assert!(
            read == Settings::read(vars(&[
                ("NO_PROXY", "straight.invalid"),
                ("http_proxy", "http://someone:secret@a:1"),
            ]))
        );
        for name in SETTINGS {
            let changed = Settings::read(|asked| {
                if asked == *name {
                    Some("http://b:2".to_string())
                } else {
                    read.var(asked)
                }
            });
            assert!(changed != read, "{name}");
        }
    }

    const EVERY_PROXY: &[(&str, &str)] = &[
        ("http_proxy", "http://127.0.0.1:7890"),
        ("HTTP_PROXY", "http://127.0.0.1:7890"),
        ("https_proxy", "http://127.0.0.1:7890"),
        ("HTTPS_PROXY", "http://127.0.0.1:7890"),
        ("all_proxy", "socks5://127.0.0.1:7890"),
        ("ALL_PROXY", "socks5://127.0.0.1:7890"),
    ];

    /// The decision's one condition: Banager's own request to the Ollama
    /// on this Mac never goes through a proxy, however the address is
    /// written, and with no `no_proxy` to say so.
    #[test]
    fn test_this_mac_is_never_reached_through_a_proxy() {
        for url in [
            "http://127.0.0.1:11434/api/tags",
            "http://127.1.2.3:11434/",
            "http://localhost:11434/api/tags",
            "http://LOCALHOST:11434/",
            "http://localhost.:11434/",
            "http://ollama.localhost:11434/",
            "http://[::1]:11434/api/tags",
            "http://[::ffff:127.0.0.1]:11434/",
            "http://0.0.0.0:11434/api/tags",
            "http://[::]:11434/",
            "https://127.0.0.1/",
        ] {
            assert_eq!(through(url, EVERY_PROXY), None, "{url}");
        }
    }

    #[test]
    fn test_a_request_goes_through_the_proxy_set_for_its_scheme_lowercase_first() {
        let crates = "https://crates.io/api/v1/crates/ripgrep";
        assert_eq!(
            through(
                crates,
                &[("https_proxy", "http://a:1"), ("HTTPS_PROXY", "http://b:2")]
            ),
            Some("http://a:1".into())
        );
        assert_eq!(
            through(crates, &[("HTTPS_PROXY", "http://b:2")]),
            Some("http://b:2".into())
        );
        // An http proxy is not one for https.
        assert_eq!(through(crates, &[("http_proxy", "http://a:1")]), None);
        // all_proxy stands in for either; an empty setting is not set.
        assert_eq!(
            through(
                crates,
                &[
                    ("https_proxy", "  "),
                    ("all_proxy", "socks5://127.0.0.1:7891")
                ]
            ),
            Some("socks5://127.0.0.1:7891".into())
        );
        assert_eq!(
            through(crates, &[("ALL_PROXY", "socks5h://proxy.lan:1080")]),
            Some("socks5h://proxy.lan:1080".into())
        );
        // An Ollama on another machine, over http.
        let remote = "http://192.168.1.20:11434/api/tags";
        assert_eq!(
            through(
                remote,
                &[("http_proxy", "http://a:1"), ("https_proxy", "http://b:2")]
            ),
            Some("http://a:1".into())
        );
        assert_eq!(through(remote, &[("https_proxy", "http://b:2")]), None);
        // Nothing set: straight.
        assert_eq!(through(crates, &[]), None);
    }

    #[test]
    fn test_no_proxy_sends_what_it_names_straight() {
        let with = |no_proxy: &str, url: &str| {
            through(
                url,
                &[
                    ("https_proxy", "http://p:1"),
                    ("http_proxy", "http://p:1"),
                    ("no_proxy", no_proxy),
                ],
            )
        };
        let crates = "https://crates.io/api/v1/crates/ripgrep";
        let ollama = "https://registry.ollama.ai/v2/library/llama3/manifests/latest";
        for no_proxy in [
            "crates.io",
            ".crates.io",
            "*.crates.io",
            " foo , crates.io ",
            "CRATES.IO",
            "*",
        ] {
            assert_eq!(with(no_proxy, crates), None, "{no_proxy}");
        }
        // A name names the names under it, not one that merely ends alike.
        assert_eq!(with("ollama.ai", ollama), None);
        assert_eq!(with("llama.ai", ollama), Some("http://p:1".into()));
        assert_eq!(with("rates.io", crates), Some("http://p:1".into()));
        // Addresses and ranges.
        let remote = "http://192.168.1.20:11434/";
        for no_proxy in [
            "192.168.1.20",
            "192.168.0.0/16",
            "10.0.0.0/8,192.168.1.0/24",
        ] {
            assert_eq!(with(no_proxy, remote), None, "{no_proxy}");
        }
        for no_proxy in [
            "192.168.2.0/24",
            "192.168.1.2",
            "192.168.1.20/40",
            "192.168.1.200",
        ] {
            assert_eq!(
                with(no_proxy, remote),
                Some("http://p:1".into()),
                "{no_proxy}"
            );
        }
        let v6 = "http://[fd00::20]:11434/";
        assert_eq!(with("fd00::/8", v6), None);
        assert_eq!(with("[fd00::20]", v6), None);
        assert_eq!(with("fe80::/10", v6), Some("http://p:1".into()));
        // NO_PROXY when no_proxy is not set.
        assert_eq!(
            through(
                crates,
                &[("https_proxy", "http://p:1"), ("NO_PROXY", "crates.io")]
            ),
            None
        );
    }
}
