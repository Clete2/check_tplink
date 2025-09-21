extern crate lazy_static;

use std::collections::HashMap;

use anyhow::{Error};
use check_tl_sg108e::tl_sg108e_stats::TPLinkStats;
use clap::Parser;
use reqwest::Client;
use regex::Regex;

// Ref: https://nagios-plugins.org/doc/guidelines.html#AEN200
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// TP Link switch hostname
    #[arg(short = 'H', long, env)]
    hostname: String,

    /// Username
    #[arg(short, long, env, default_value = "admin")]
    username: String,

    /// Authentication password
    #[arg(short, long, env, default_value = "admin")]
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

    let password_file = ".".to_owned() + &args.hostname + "_password";

    let password_from_file = 
        std::fs::read_to_string(password_file);
    let password = if let Some(p) = password_from_file.ok() {
        p
    } else if let Some(authentication) = args.authentication {
        //"No password provided and could not read password out of `.**hostname**_password` file."
        //fallback to passed arg or default admin login
        authentication
     } else {
        "".to_owned()
    };

    let response = get_statistics(&args.hostname, &args.username, &password).await?;

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
    //params.insert("cpassword", "");// unused/breaks on tl_sg108e
    params.insert("logon", "Login");

    let response = client
        .post(format!("http://{}/logon.cgi", hostname))
        .form(&params)
        .send()
        .await?;

    // returns 401 on success, use response text.
    let response_text = response.text().await?;

    //success:
    /*
    <script>
var logonInfo = new Array(
0,
0,0);
var g_Lan = 460;
var g_year=2017;
</script>
...
 */

//fail:
/*<script>
    var logonInfo = new Array(
        1,
        0, 0);
    var g_Lan = 460;
    var g_year = 2017;
</script> */
/*var errType = logonInfo[0];
<SPAN class="WARN_NORMAL" id="t_error1">The user name or the password is wrong.</SPAN>',
<SPAN class="WARN_NORMAL" id="t_error2">The user is not allowed to login.</SPAN>
<SPAN class="WARN_NORMAL" id="t_error3">The number of the user that allowed to login has been full.</SPAN>';
<SPAN class="WARN_NORMAL" id="t_error4">The number of the login user has been full,it is allowed 16 people to login at the same time.</SPAN>'
<SPAN class="WARN_NORMAL" id="t_error5">The session is timeout.<br>Please login again.</SPAN>'
*/
    let login_status: u8 = Regex::new(r"var logonInfo = new Array\(\n\s*([0,1,2,3,4,5]),")
    .expect("logon status regex failed to compile")
    .captures(&response_text)
    .expect("login status regex failed to capture")
    .get(1)
    .expect("login status regex didn't have enought matches")
    .as_str()
    .parse()?;
    match login_status {
        0 => Ok(()),
        1 => Err(Error::msg("The user name or the password is wrong.")),
        2 => Err(Error::msg("The user is not allowed to login.")),
        3 => Err(Error::msg("The number of the user that allowed to login has been full.")),
        4 => Err(Error::msg("The number of the login user has been full,it is allowed 16 people to login at the same time.")),
        5 => Err(Error::msg("The session is timeout.<br>Please login again.")),
        6_u8..=u8::MAX => Err(Error::msg("unpossible login error"))
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
