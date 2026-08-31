use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::builder::ValueParser;
use clap::{App, AppSettings, Arg};
use tokio::io::AsyncWriteExt;
use tokio::net::{UdpSocket, UnixListener};
use turn::Error;
use turn::auth::generate_long_term_credentials;
use turn::auth::*;
use turn::relay::relay_range::RelayAddressGeneratorRanges;
use turn::server::Server;
use turn::server::config::{ConnConfig, ServerConfig};
use webrtc_util::vnet::net::Net;

mod cli;

fn listen_ips() -> BTreeSet<IpAddr> {
    let mut ip_set = BTreeSet::new();
    let interfaces = netdev::interface::get_interfaces();
    for interface in interfaces {
        for ip in interface.ip_addrs() {
            if !ip.is_loopback() && !is_link_local(ip) {
                ip_set.insert(ip);
            }
        }
    }
    ip_set
}

/// Link-local addresses (fe80::/10 in IPv6, 169.254.0.0/16 in IPv4) are non-routable
/// and should be excluded from TURN listening addresses because:
/// 1. They are only reachable within the same network segment.
/// 2. Binding to an IPv6 link-local address requires a Scope ID (interface index),
///    otherwise the OS returns EINVAL (Invalid Argument).
fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_link_local(),
        IpAddr::V6(ipv6) => ipv6.is_unicast_link_local(),
    }
}

async fn create_conn_config(
    listen_ip: IpAddr,
    conn: Option<Arc<UdpSocket>>,
    listen: &cli::ListenCfg,
    relay: &cli::RelayCfg,
) -> Result<ConnConfig, Error> {
    println!("Listening on public IP: {listen_ip}");
    let conn = match conn {
        Some(conn) => conn, // listener socket with user-specified host already created
        None => Arc::new(UdpSocket::bind((listen_ip, listen.port)).await?),
    };
    let relay_ip = relay.ip.unwrap_or(listen_ip);
    Ok(ConnConfig {
        conn,
        relay_addr_generator: Box::new(RelayAddressGeneratorRanges {
            relay_address: relay_ip,
            address: relay_ip.to_string(),
            min_port: relay.min_port,
            max_port: relay.max_port,
            max_retries: 0, // use the default
            net: Arc::new(Net::new(None)),
        }),
    })
}

/// Listens on the Unix socket,
/// returning valid credentials to any connecting client.
async fn socket_loop(path: &Path, shared_secret: &str) -> Result<()> {
    let listener = UnixListener::bind(path).context("Failed to bind Unix socket")?;
    loop {
        match listener.accept().await {
            Ok((mut stream, _addr)) => {
                let duration = Duration::from_secs(5 * 24 * 3600);
                let (username, password) = generate_long_term_credentials(shared_secret, duration)?;

                // Write credentials to stdout.
                // Newline indicates the end of the answer
                // and allows the client to tell if the answer
                // was truncated if the server is restarted
                // or crashed while writing the answer.
                let res = format!("{username}:{password}\n");
                stream.write_all(res.as_bytes()).await?;
            }
            Err(err) => {
                eprintln!("Unix connection failed: {err}.");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let mut app = App::new("TURN Server UDP")
        .about("Chatmail TURN Server UDP")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::new("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::new("realm")
                .default_value("webrtc.rs")
                .takes_value(true)
                .long("realm")
                .help("Realm (defaults to \"webrtc.rs\")"),
        )
        .arg(
            Arg::new("socket")
                .required(true)
                .takes_value(true)
                .long("socket")
                .help("Unix socket path"),
        )
        .arg(
            Arg::new("listen")
                .default_value(":3478")
                .takes_value(true)
                .value_parser(ValueParser::new(cli::parse_listen))
                .long("listen")
                .help("Address to bind TURN listener to: [ip]:<port>"),
        )
        .arg(
            Arg::new("relayaddr")
                .default_value(":49152-65535")
                .takes_value(true)
                .value_parser(ValueParser::new(cli::parse_range))
                .long("relay-addr")
                .help("Host and port range available for TURN relay: [ip]:<min>-<max>"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let realm = matches.value_of("realm").unwrap();
    let socket_path = Path::new(matches.value_of("socket").unwrap());
    let listen: cli::ListenCfg = *matches.get_one("listen").unwrap();
    let relay: cli::RelayCfg = *matches.get_one("relayaddr").unwrap();

    let conn = match listen.ip {
        Some(ip) => Some(Arc::new(UdpSocket::bind((ip, listen.port)).await?)),
        _ => None,
    };

    let conn_configs = if conn.is_some() && relay.ip.is_some() {
        // do not iterate over available IPs
        // when both hosts are explicitly specified
        vec![create_conn_config(listen.ip.unwrap(), conn, &listen, &relay).await?]
    } else {
        let mut conn_configs = Vec::new();
        for listen_ip in listen_ips() {
            conn_configs.push(create_conn_config(listen_ip, conn.clone(), &listen, &relay).await?);
        }
        conn_configs
    };

    let shared_secret = "north";
    let auth_handler = LongTermAuthHandler::new(shared_secret.to_string());

    let server = Server::new(ServerConfig {
        conn_configs,
        realm: realm.to_owned(),
        auth_handler: Arc::new(auth_handler),
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await?;

    socket_loop(Path::new(socket_path), shared_secret)
        .await
        .unwrap();

    server.close().await?;

    Ok(())
}
