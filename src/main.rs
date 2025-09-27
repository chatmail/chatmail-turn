use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use clap::{App, AppSettings, Arg};
use tokio::net::UdpSocket;
use tokio::signal;
use tokio::time::Duration;
use turn::auth::*;
use turn::relay::relay_static::RelayAddressGeneratorStatic;
use turn::server::config::{ServerConfig, ConnConfig};
use turn::server::Server;
use turn::Error;
use webrtc_util::vnet::net::Net;

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
            Arg::with_name("public-ip")
                .required_unless("FULLHELP")
                .takes_value(true)
                .long("public-ip")
                .help("IP Address that TURN can be contacted by."),
        )
        .arg(
            Arg::with_name("realm")
                .default_value("webrtc.rs")
                .takes_value(true)
                .long("realm")
                .help("Realm (defaults to \"webrtc.rs\")"),
        )
        .arg(
            Arg::with_name("port")
                .takes_value(true)
                .default_value("3478")
                .long("port")
                .help("Listening port."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let public_ip = matches.value_of("public-ip").unwrap();
    let port = matches.value_of("port").unwrap();
    let realm = matches.value_of("realm").unwrap();

    let conn = Arc::new(UdpSocket::bind(format!("0.0.0.0:{port}")).await?);

    let auth_handler = LongTermAuthHandler::new("north".to_string());

    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: IpAddr::from_str(public_ip)?,
                address: "0.0.0.0".to_owned(),
                net: Arc::new(Net::new(None)),
            }),
        }],
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
