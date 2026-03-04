use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::{App, AppSettings, Arg, value_parser};
use tokio::io::AsyncWriteExt;
use tokio::net::{UdpSocket, UnixListener};
use turn::Error;
use turn::auth::generate_long_term_credentials;
use turn::auth::*;
use turn::relay::relay_static::RelayAddressGeneratorStatic;
use turn::relay::relay_range::RelayAddressGeneratorRanges;
use turn::server::Server;
use turn::server::config::{ConnConfig, ServerConfig};
use webrtc_util::vnet::net::Net;

fn public_ips() -> BTreeSet<IpAddr> {
    let mut ip_set = BTreeSet::new();
    let interfaces = netdev::interface::get_interfaces();
    for interface in interfaces {
        ip_set.extend(interface.global_ip_addrs());
    }
    ip_set
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
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("realm")
                .default_value("webrtc.rs")
                .takes_value(true)
                .long("realm")
                .help("Realm (defaults to \"webrtc.rs\")"),
        )
        .arg(
            Arg::with_name("socket")
                .required(true)
                .takes_value(true)
                .long("socket")
                .value_parser(value_parser!(PathBuf))
                .help("Unix socket path"),
        )
        .arg(
            Arg::with_name("listenport")
                .default_value("3478")
                .takes_value(true)
                .long("listen-port")
                .value_parser(value_parser!(u16))
                .help("UDP listen port for incoming connections"),
        )
        .arg(
            Arg::with_name("listenaddress")
                .required(false)
                .takes_value(true)
                .long("listen-address")
                .value_parser(value_parser!(Ipv4Addr))
                .help("Local listen ipv4 address used together with --relay-address to run behind a nat"),
        )
        .arg(
            Arg::with_name("relayaddress")
                .required(false)
                .takes_value(true)
                .long("relay-address")
                .value_parser(value_parser!(Ipv4Addr))
                .help("External relay ipv4 address used together with --listen-address to run behind a nat"),
        )
        .arg(
            Arg::with_name("minrelayport")
                .default_value("49152")
                .takes_value(true)
                .long("min-relay-port")
                .value_parser(value_parser!(u16))
                .help("Minimum UDP port for relay connections (used only when --listen-address and --relay-address are specified)"),
        )
        .arg(
            Arg::with_name("maxrelayport")
                .default_value("65535")
                .takes_value(true)
                .long("max-relay-port")
                .value_parser(value_parser!(u16))
                .help("Maximum UDP port for relay connections (used only when --listen-address and --relay-address are specified)"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let port = *matches.get_one::<u16>("listenport").unwrap();
    let realm = matches.value_of("realm").unwrap();
    let socket_path = Path::new(matches.get_one::<PathBuf>("socket").unwrap());

    let mut conn_configs = Vec::new();
    if matches.is_present("relayaddress") & matches.is_present("listenaddress") {
        let external_ip = IpAddr::V4(*matches.get_one::<Ipv4Addr>("relayaddress").expect("Invalid address"));
        let local_ip = IpAddr::V4(*matches.get_one::<Ipv4Addr>("listenaddress").expect("Invalid address"));
        let min_relay_port = *matches.get_one::<u16>("minrelayport").unwrap();
        let max_relay_port = *matches.get_one::<u16>("maxrelayport").unwrap();
        println!("Listening on local IP: {local_ip}");
        let conn = Arc::new(UdpSocket::bind((local_ip, port)).await?);
        let conn_config = ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorRanges {
                relay_address: external_ip,
                min_port: min_relay_port,
                max_port: max_relay_port,
                max_retries: 10,
                address: local_ip.to_string(),
                net: Arc::new(Net::new(None)),
            }),
        };
        conn_configs.push(conn_config);
    }
    for public_ip in public_ips() {
        println!("Listening on public IP: {public_ip}");
        let conn = Arc::new(UdpSocket::bind((public_ip, port)).await?);
        let conn_config = ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: public_ip,
                address: public_ip.to_string(),
                net: Arc::new(Net::new(None)),
            }),
        };
        conn_configs.push(conn_config);
    }

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
