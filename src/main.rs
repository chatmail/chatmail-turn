use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::Arc;

use clap::{App, AppSettings, Arg};
use tokio::net::UdpSocket;
use tokio::signal;
use tokio::time::Duration;
use turn::Error;
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
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let port = 3478;
    let realm = matches.value_of("realm").unwrap();

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

    let auth_handler = LongTermAuthHandler::new("north".to_string());

    let server = Server::new(ServerConfig {
        conn_configs,
        realm: realm.to_owned(),
        auth_handler: Arc::new(auth_handler),
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await?;

    println!("Waiting for Ctrl-C...");
    signal::ctrl_c().await.expect("failed to listen for event");
    server.close().await?;

    Ok(())
}
