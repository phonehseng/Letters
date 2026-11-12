//! reading a server address the person typed, and finding out whether it
//! really points at this computer.
//!
//! "does this stay on the machine" is decided from the addresses a name
//! resolves to, never from how the name looks: `localhost` could be
//! pointed elsewhere in a hosts file, and `my-box.local` could be this
//! computer or another one.

use std::io;
use std::net::{IpAddr, ToSocketAddrs};

use crate::AiError;

/// the parts of an address such as `https://api.example.com/v1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub secure: bool,
    /// the host as typed, without brackets around an ipv6 address.
    pub host: String,
    pub port: u16,
    /// the path, without a trailing slash ("" or "/v1").
    pub path: String,
}

impl Address {
    /// reads an address, refusing anything unusual.
    ///
    /// user names, passwords, queries and fragments are refused rather than
    /// ignored, because each could carry something the person didn't mean
    /// to send (a password in the address would end up in the request).
    pub fn parse(text: &str) -> Result<Address, AiError> {
        let text = text.trim();
        let (secure, rest) = split_scheme(text)?;
        if rest.contains(['?', '#']) {
            return Err(invalid("remove the part after ? or #"));
        }
        let (authority, path) = match rest.find('/') {
            Some(slash) => (&rest[..slash], &rest[slash..]),
            None => (rest, ""),
        };
        if authority.contains('@') {
            return Err(invalid("remove the user name or password from it"));
        }
        let default_port = if secure { 443 } else { 80 };
        let (host, port) = split_host_port(authority, default_port)?;
        Ok(Address {
            secure,
            host,
            port,
            path: path.trim_end_matches('/').to_string(),
        })
    }

    /// the host the way it goes into a url, with brackets for ipv6.
    pub fn host_for_url(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }

    /// a full url for one endpoint below this address.
    pub fn url(&self, endpoint: &str) -> String {
        self.url_with_host(&self.host_for_url(), endpoint)
    }

    /// the same url, but aimed at an exact ip address instead of the name.
    ///
    /// used for the local server: once the name has been checked to mean
    /// this computer, connecting to the checked ip makes sure a later
    /// lookup can't quietly point somewhere else.
    pub fn url_for_ip(&self, ip: IpAddr, endpoint: &str) -> String {
        let host = match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        self.url_with_host(&host, endpoint)
    }

    fn url_with_host(&self, host: &str, endpoint: &str) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{host}:{}{}{endpoint}", self.port, self.path)
    }
}

fn split_scheme(text: &str) -> Result<(bool, &str), AiError> {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("https://") {
        Ok((true, &text[8..]))
    } else if lower.starts_with("http://") {
        Ok((false, &text[7..]))
    } else {
        Err(invalid("it should start with https:// or http://"))
    }
}

fn split_host_port(authority: &str, default_port: u16) -> Result<(String, u16), AiError> {
    // an ipv6 address is written in brackets so its colons aren't
    // mistaken for the port separator
    let (host, port_text) = if let Some(inner) = authority.strip_prefix('[') {
        let close = inner.find(']').ok_or_else(|| invalid("a ] is missing"))?;
        let after = &inner[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port),
            None if after.is_empty() => None,
            None => return Err(invalid("check the part after the ]")),
        };
        (&inner[..close], port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(invalid("the server name is missing"));
    }
    let port = match port_text {
        None => default_port,
        Some(port) => port
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| invalid("the port number should be between 1 and 65535"))?,
    };
    Ok((host.to_ascii_lowercase(), port))
}

fn invalid(reason: &str) -> AiError {
    AiError::InvalidAddress(format!("{reason}."))
}

/// turns a host name into ip addresses.
///
/// a trait so tests can say "this name means 10.0.0.5" without any
/// network lookup.
pub trait Resolver {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<IpAddr>>;
}

/// the operating system's own lookup (hosts file, then dns).
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<IpAddr>> {
        let addresses = (host, port).to_socket_addrs()?;
        Ok(addresses.map(|address| address.ip()).collect())
    }
}

/// where an address leads, once looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub addresses: Vec<IpAddr>,
    /// true only when every address the name has is this computer.
    pub loopback: bool,
}

/// looks a host up and decides whether it is this computer.
///
/// one address elsewhere is enough to say no: the connection might use it.
pub fn resolve(address: &Address, resolver: &dyn Resolver) -> Result<Resolved, AiError> {
    let addresses = resolver
        .resolve(&address.host, address.port)
        .map_err(|_| AiError::NotReachable(address.host.clone()))?;
    if addresses.is_empty() {
        return Err(AiError::NotReachable(address.host.clone()));
    }
    let loopback = addresses.iter().all(is_loopback);
    Ok(Resolved {
        addresses,
        loopback,
    })
}

/// true for 127.x.x.x, ::1 and the ipv6 spelling of 127.x.x.x.
pub fn is_loopback(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// a lookup table standing in for dns.
    struct FakeResolver(HashMap<&'static str, Vec<IpAddr>>);

    impl Resolver for FakeResolver {
        fn resolve(&self, host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
            self.0
                .get(host)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown"))
        }
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn parses_typical_addresses() {
        let a = Address::parse("https://api.example.com/v1/").unwrap();
        assert_eq!(
            (a.secure, a.host.as_str(), a.port, a.path.as_str()),
            (true, "api.example.com", 443, "/v1")
        );
        let b = Address::parse("http://127.0.0.1:8080").unwrap();
        assert_eq!((b.secure, b.port, b.path.as_str()), (false, 8080, ""));
        let c = Address::parse("http://[::1]:9000/x").unwrap();
        assert_eq!(c.host, "::1");
        assert_eq!(c.url("/health"), "http://[::1]:9000/x/health");
    }

    #[test]
    fn refuses_odd_addresses() {
        for bad in [
            "ftp://example.com",
            "example.com",
            "https://user:pw@example.com",
            "https://example.com/?key=1",
            "https://example.com:0",
            "https://example.com:99999",
            "https://:80",
            "http://[::1",
        ] {
            assert!(Address::parse(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn literal_loopback_addresses_stay_on_the_computer() {
        let resolver = SystemResolver;
        for text in [
            "http://127.0.0.1:1",
            "http://[::1]:1",
            "http://127.8.9.10:1",
        ] {
            let resolved = resolve(&Address::parse(text).unwrap(), &resolver).unwrap();
            assert!(resolved.loopback, "{text}");
        }
    }

    #[test]
    fn localhost_resolves_to_this_computer() {
        // the hosts file answers this, so no network is involved
        let resolved = resolve(
            &Address::parse("http://localhost:1").unwrap(),
            &SystemResolver,
        )
        .unwrap();
        assert!(resolved.loopback);
    }

    #[test]
    fn a_name_is_judged_by_where_it_resolves() {
        let resolver = FakeResolver(HashMap::from([
            ("localhost", vec![ip("10.0.0.5")]),
            ("my-box", vec![ip("127.0.0.1"), ip("192.168.1.20")]),
            ("home-server", vec![ip("::ffff:127.0.0.1")]),
        ]));
        let check = |text: &str| {
            resolve(&Address::parse(text).unwrap(), &resolver)
                .unwrap()
                .loopback
        };
        // a hosts file that points localhost elsewhere is believed
        assert!(!check("http://localhost:8080"));
        // one address off the computer is enough to say it leaves
        assert!(!check("http://my-box:8080"));
        assert!(check("http://home-server:8080"));
    }

    #[test]
    fn an_unknown_name_is_not_reachable() {
        let resolver = FakeResolver(HashMap::new());
        let error =
            resolve(&Address::parse("http://nowhere.test").unwrap(), &resolver).unwrap_err();
        assert_eq!(error, AiError::NotReachable("nowhere.test".into()));
    }

    #[test]
    fn url_for_ip_brackets_ipv6() {
        let a = Address::parse("http://localhost:8080").unwrap();
        assert_eq!(a.url_for_ip(ip("::1"), "/v1/x"), "http://[::1]:8080/v1/x");
        assert_eq!(
            a.url_for_ip(ip("127.0.0.1"), "/v1/x"),
            "http://127.0.0.1:8080/v1/x"
        );
    }
}
