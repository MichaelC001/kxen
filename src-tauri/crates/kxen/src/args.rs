//! kxen 命令行参数（手工解析，零新依赖）。

use std::net::{IpAddr, Ipv4Addr};

const DEFAULT_PORT: u16 = 7824;

pub struct Cli {
    pub bind: IpAddr,
    pub port: u16,
    pub token: Option<String>,
    pub allow_hosts: Vec<String>,
}

pub enum Parsed {
    Run(Cli),
    Help,
}

pub const HELP: &str = "\
kxen - headless web server (no GUI, full app service in a browser)

USAGE:
    kxen [OPTIONS]

OPTIONS:
    --bind <IP>          listen address (default 127.0.0.1; non-loopback exposes the LAN)
    --port <PORT>        listen port (default 7824; exits with an error if occupied)
    --token <TOKEN>      fixed WS handshake token (default: random per start; fix it to bookmark the URL)
    --allow-host <HOST>  add a Host header whitelist entry (repeatable; e.g. a tailscale hostname)
    -h, --help           print this help

ENV:
    KXEN_DATA_DIR        override the data directory (goals/sessions/auth.json)
    RUST_LOG             log filter (tracing env-filter)

ACCESS:
    the full URL with token is printed on startup; the token is the only auth, keep it secret.
    remote access: terminate TLS with `tailscale serve` and pass --allow-host <your-tailnet-host>.
";

pub fn parse(args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut bind = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut port = DEFAULT_PORT;
    let mut token = None;
    let mut allow_hosts = Vec::new();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--bind" => {
                let value = value_of(&mut args, "--bind")?;
                bind = value.parse().map_err(|_| format!("--bind expects an IP address, got {value:?}"))?;
            }
            "--port" => {
                let value = value_of(&mut args, "--port")?;
                port = value.parse().map_err(|_| format!("--port expects a u16 port, got {value:?}"))?;
            }
            "--token" => {
                let value = value_of(&mut args, "--token")?;
                if value.is_empty() {
                    return Err("--token expects a non-empty string".to_string());
                }
                token = Some(value);
            }
            "--allow-host" => {
                let value = value_of(&mut args, "--allow-host")?;
                let host = value.trim();
                if host.is_empty() || host.chars().any(char::is_whitespace) || host.contains('/') {
                    return Err(format!("--allow-host expects a bare hostname, got {value:?}"));
                }
                allow_hosts.push(host.to_string());
            }
            unknown => return Err(format!("unknown flag {unknown:?} (see --help)")),
        }
    }
    Ok(Parsed::Run(Cli { bind, port, token, allow_hosts }))
}

fn value_of(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} expects a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Cli, String> {
        match parse(args.iter().map(|arg| arg.to_string()))? {
            Parsed::Run(cli) => Ok(cli),
            Parsed::Help => Err("unexpected help".to_string()),
        }
    }

    #[test]
    fn defaults_are_loopback_and_preferred_port() {
        let cli = run(&[]).unwrap();
        assert_eq!(cli.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(cli.port, 7824);
        assert_eq!(cli.token, None);
        assert!(cli.allow_hosts.is_empty());
    }

    #[test]
    fn parses_all_flags() {
        let cli = run(&["--bind", "0.0.0.0", "--port", "9000", "--token", "abc", "--allow-host", "a.ts.net", "--allow-host", "b.ts.net"])
            .unwrap();
        assert_eq!(cli.bind, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(cli.port, 9000);
        assert_eq!(cli.token.as_deref(), Some("abc"));
        assert_eq!(cli.allow_hosts, vec!["a.ts.net", "b.ts.net"]);
    }

    #[test]
    fn rejects_bad_values() {
        assert!(run(&["--bind", "not-an-ip"]).is_err());
        assert!(run(&["--port", "99999"]).is_err());
        assert!(run(&["--port"]).is_err());
        assert!(run(&["--token", ""]).is_err());
        assert!(run(&["--allow-host", "evil host"]).is_err());
        assert!(run(&["--allow-host", "http://x"]).is_err());
        assert!(run(&["--bogus"]).is_err());
    }

    #[test]
    fn help_short_circuits() {
        for flag in ["-h", "--help"] {
            let parsed = parse([flag.to_string()].into_iter()).unwrap();
            assert!(matches!(parsed, Parsed::Help));
        }
    }
}
