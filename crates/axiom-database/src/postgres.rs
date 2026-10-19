//! PostgreSQL target and transport policy.
//!
//! The offline build has no PostgreSQL driver. This module still validates
//! every configured target: remote connections require TLS verification and
//! private/link-local/metadata addresses are refused unless explicitly allowed.
//! Release CI may supply `AXIOM_TEST_POSTGRES_URL`; the gated test exercises
//! this policy and skips when the variable is absent.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::DbError;

/// A parsed PostgreSQL target. Passwords are not retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresTarget {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: Option<String>,
    pub sslmode: SslMode,
}

/// Accepted TLS modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    Require,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    fn parse(value: &str) -> Option<SslMode> {
        Some(match value {
            "require" => SslMode::Require,
            "verify-ca" => SslMode::VerifyCa,
            "verify-full" => SslMode::VerifyFull,
            _ => return None,
        })
    }

    /// Stable name.
    pub fn as_str(self) -> &'static str {
        match self {
            SslMode::Require => "require",
            SslMode::VerifyCa => "verify-ca",
            SslMode::VerifyFull => "verify-full",
        }
    }
}

/// Connection setup policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PostgresPolicy {
    /// Permit loopback/private/link-local targets. Defaults to `false`.
    pub allow_private_network: bool,
}

impl PostgresPolicy {
    /// Validate and parse a PostgreSQL URL.
    pub fn parse(&self, url: &str) -> Result<PostgresTarget, DbError> {
        let rest = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .ok_or_else(|| DbError::Config("PostgreSQL URL must use postgres://".into()))?;
        let (authority, path_and_query) = rest
            .split_once('/')
            .ok_or_else(|| DbError::Config("PostgreSQL URL must include a database name".into()))?;
        let (userinfo, host_port) = match authority.rsplit_once('@') {
            Some((userinfo, host_port)) => (Some(userinfo), host_port),
            None => (None, authority),
        };
        let user = userinfo
            .and_then(|userinfo| userinfo.split_once(':').map(|(user, _)| user))
            .or(userinfo)
            .filter(|user| !user.is_empty())
            .map(str::to_string);

        let (host, port) = parse_host_port(host_port)?;
        let (database, query) = path_and_query
            .split_once('?')
            .map_or((path_and_query, ""), |(database, query)| (database, query));
        if database.is_empty() {
            return Err(DbError::Config(
                "PostgreSQL URL must include a database name".into(),
            ));
        }

        let mut sslmode = None;
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                if key == "sslmode" {
                    sslmode = SslMode::parse(value);
                }
            }
        }
        let Some(sslmode) = sslmode else {
            return Err(DbError::Config(
                "remote PostgreSQL requires sslmode=require|verify-ca|verify-full".into(),
            ));
        };
        if !self.allow_private_network && is_private(&host) {
            return Err(DbError::Config(
                "private or link-local PostgreSQL targets are refused".into(),
            ));
        }

        Ok(PostgresTarget {
            host,
            port,
            database: database.to_string(),
            user,
            sslmode,
        })
    }
}

impl PostgresTarget {
    /// A redacted URL safe for logs and diagnostics.
    pub fn sanitized(&self) -> String {
        let user = self
            .user
            .as_ref()
            .map(|user| format!("{user}@"))
            .unwrap_or_default();
        format!(
            "postgres://{user}{}:{}/{}?sslmode={}",
            self.host,
            self.port,
            self.database,
            self.sslmode.as_str()
        )
    }
}

fn parse_host_port(text: &str) -> Result<(String, u16), DbError> {
    if let Some(rest) = text.strip_prefix('[') {
        let (host, rest) = rest
            .split_once(']')
            .ok_or_else(|| DbError::Config("invalid IPv6 PostgreSQL target".into()))?;
        let port = match rest.strip_prefix(':') {
            Some(port) => port
                .parse()
                .map_err(|_| DbError::Config("invalid PostgreSQL port".into()))?,
            None => 5432,
        };
        return Ok((host.to_string(), port));
    }
    match text.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Ok((
            host.to_string(),
            port.parse()
                .map_err(|_| DbError::Config("invalid PostgreSQL port".into()))?,
        )),
        Some(_) => Err(DbError::Config("missing PostgreSQL host".into())),
        None if !text.is_empty() => Ok((text.to_string(), 5432)),
        None => Err(DbError::Config("missing PostgreSQL host".into())),
    }
}

fn is_private(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost"
        || lower == "metadata.google.internal"
        || lower.ends_with(".internal")
        || lower == "host.docker.internal"
    {
        return true;
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        return match address {
            IpAddr::V4(address) => is_private_v4(address),
            IpAddr::V6(address) => is_private_v6(address),
        };
    }
    false
}

fn is_private_v4(address: Ipv4Addr) -> bool {
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_unspecified()
        || address.octets()[0] == 0
}

fn is_private_v6(address: Ipv6Addr) -> bool {
    address.is_loopback()
        || address.is_unspecified()
        || (address.segments()[0] & 0xfe00) == 0xfc00
        || (address.segments()[0] & 0xffc0) == 0xfe80
}
