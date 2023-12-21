extern crate lazy_static;

use std::collections::HashMap;

use anyhow::{anyhow, Error};
use check_tplink::tplink_stats::TPLinkStats;
use clap::Parser;
use reqwest::Client;

// Ref: https://nagios-plugins.org/doc/guidelines.html#AEN200
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// TP Link switch hostname
    #[arg(short = 'H', long, env)]
    hostname: String,

    /// Username
    #[arg(short, long, env, default_value = "admin")]
    logname: String,

    /// Authentication password
    #[arg(short, long, env)]
    authentication: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    match process().await {
        Ok(result) => {
            println!("{}", result);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn process() -> Result<String, Error> {
    let args = Args::parse();

    let password = if let Some(authentication) = args.authentication {
        authentication
    } else {
        std::fs::read_to_string(".tplink").or(Err(anyhow!(
            "No password provided and could not read password out of `.tplink` file."
        )))?
    };

    let response = get_statistics(&args.hostname, &args.logname, &password).await?;

    let status: TPLinkStats = response.try_into()?;
    let status = status.port_statistics;

    let mut total_good_tx: u128 = 0;
    let mut total_bad_tx: u128 = 0;
    let mut total_good_rx: u128 = 0;
    let mut total_bad_rx: u128 = 0;
    let total_ports = status.len();

    let mut num_ports_connected = 0;
    for s in &status {
        if s.link_status.is_connected() {
            num_ports_connected += 1;
        }
    }

    let mut response = format!(
        "OK: ports connected: {}/{} |",
        num_ports_connected, total_ports
    );

    for stat in status.into_iter() {
        let port_name = format!(" Port{}", stat.port_number);
        let link_speed_bytes = (stat.link_status.as_int() as usize / 8) * 1000 * 1000; // Megabits to bytes

        response.push_str(&format!("{}Enabled={}", &port_name, stat.enabled as u8));
        response.push_str(&format!("{}LinkSpeed={}B", &port_name, link_speed_bytes));
        response.push_str(&format!("{}GoodTX={}c", &port_name, stat.tx_good_packets));
        response.push_str(&format!("{}BadTX={}c", &port_name, stat.tx_bad_packets));
        response.push_str(&format!("{}GoodRX={}c", &port_name, stat.rx_good_packets));
        response.push_str(&format!("{}BadRX={}c", &port_name, stat.rx_bad_packets));

        total_good_tx += stat.tx_good_packets;
        total_bad_tx += stat.tx_bad_packets;
        total_good_rx += stat.rx_good_packets;
        total_bad_rx += stat.rx_bad_packets;
    }

    response.push_str(&format!(
        " TotalGoodTX={}c TotalBadTX={}c TotalGoodRX={}c TotalBadRX={}c PortsConnected={} TotalPorts={}",
        total_good_tx, total_bad_tx, total_good_rx, total_bad_rx, num_ports_connected, total_ports
    ));

    Ok(response)
}

async fn login(
    client: &Client,
    hostname: &str,
    username: &str,
    password: &str,
) -> Result<(), Error> {
    let mut params = HashMap::new();
    params.insert("username", username);
    params.insert("password", password);
    params.insert("cpassword", "");
    params.insert("logon", "Login");

    let response = client
        .post(format!("http://{}/logon.cgi", hostname))
        .form(&params)
        .send()
        .await?;

    let response = response.text().await?;

    match response.contains("logonInfo") {
        true => Ok(()),
        false => Err(Error::msg("Could not login. Check the configuration.")),
    }
}

async fn get_statistics(hostname: &str, username: &str, password: &str) -> Result<String, Error> {
    let client = reqwest::Client::new();

    // I'd rather try and get the data in the beginning than login each time
    // The assumption here is that this script is run on a cron basis
    // and almost always the user will already be logged in (see comment below)
    let response = client
        .get(format!("http://{}/PortStatisticsRpm.htm", hostname))
        .send()
        .await?
        .text()
        .await?;

    // TPLink is very insecure and does not even require cookies to login
    // if you logged in from this IP before and it remembers you then you will
    // stay logged in for some time.
    if response.contains("max_port_num") {
        return Ok(response);
    }

    login(&client, hostname, username, password).await?;

    // TODO: This is duplicate of first line of this function
    Ok(client
        .get(format!("http://{}/PortStatisticsRpm.htm", hostname))
        .send()
        .await?
        .text()
        .await?)
}
