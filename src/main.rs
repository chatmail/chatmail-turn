use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::{App, AppSettings, Arg};
use tokio::io::AsyncWriteExt;
use tokio::net::{UdpSocket, UnixListener};
use turn::Error;
use turn::auth::generate_long_term_credentials;
use turn::auth::*;
use turn::relay::relay_static::RelayAddressGeneratorStatic;
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
                let res = format!("{username}:{password}");
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
                .help("Unix socket path"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let port = 3478;
    let realm = matches.value_of("realm").unwrap();
    let socket_path = Path::new(matches.value_of("socket").unwrap());

    let mut conn_configs = Vec::new();
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
