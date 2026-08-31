use std::net::{IpAddr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenCfg {
    pub ip: Option<IpAddr>,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayCfg {
    pub ip: Option<IpAddr>,
    pub min_port: u16,
    pub max_port: u16,
}

fn parse_ip(val: &str) -> Result<(Option<IpAddr>, &str), &'static str> {
    let val = val.trim();
    match val.as_bytes().first() {
        Some(b'[') => {
            let closing = val.find(']').ok_or("not a valid ipv6")?;
            let v6 = val[1..closing]
                .parse::<Ipv6Addr>()
                .map_err(|_| "not a valid ipv6")?;
            if val.get(closing + 1..closing + 2) == Some(":") {
                Ok((Some(IpAddr::V6(v6)), &val[closing + 2..]))
            } else {
                Err("no port specified")
            }
        }
        Some(b':') => Ok((None, &val[1..])),
        _ => {
            let (ip, port) = val
                .split_once(':')
                .ok_or("format must be ip:port or :port")?;
            let ip = ip.parse::<IpAddr>().map_err(|_| "not a valid ip")?;
            Ok((Some(ip), port))
        }
    }
}

pub fn parse_listen(val: &str) -> Result<ListenCfg, &'static str> {
    let (ip, port) = parse_ip(val)?;
    let port = port.parse().map_err(|_| "cannot parse port")?;
    Ok(ListenCfg { ip, port })
}

pub fn parse_range(val: &str) -> Result<RelayCfg, &'static str> {
    let (ip, range) = parse_ip(val)?;
    let (min, max) = range.split_once('-').ok_or("cannot parse as range")?;
    Ok(RelayCfg {
        ip,
        min_port: min.parse().map_err(|_| "cannot parse min port")?,
        max_port: max.parse().map_err(|_| "cannot parse max port")?,
    })
}
