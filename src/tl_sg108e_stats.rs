use anyhow::{anyhow, Error};
use lazy_static::lazy_static;
use regex::Regex;

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct TPLinkStats {
    pub port_statistics: Vec<PortStatistic>,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct PortStatistic {
    pub port_number: u8,
    pub enabled: bool,
    pub link_status: LinkStatus,
    pub tx_good_packets: u128,
    pub tx_bad_packets: u128,
    pub rx_good_packets: u128,
    pub rx_bad_packets: u128,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum LinkStatus {
    Down,
    Auto,
    TenHalf,
    TenFull,
    OneHundredHalf,
    OneHundredFull,
    OneThousandFull,
    Empty,
}

lazy_static! {
    static ref NUM_PORTS: Regex =
        Regex::new(r"max_port_num\s=\s(\d+)").expect("Port number regex did not compile.");
    static ref STATE: Regex =
        Regex::new(r"state:\[(.*?)\],").expect("State regex did not compile.");
    static ref LINK_STATUS: Regex =
        Regex::new(r"link_status:\[(.*?)\],").expect("Link status regex did not compile.");
    static ref PACKETS: Regex =
        Regex::new(r"pkts:\[(.*?)\]").expect("Packet count regex did not compile.");
}

impl TryInto<TPLinkStats> for String {
    type Error = Error;

    fn try_into(self) -> Result<TPLinkStats, Self::Error> {
        let num_ports: usize = NUM_PORTS
            .captures(&self)
            .expect("No port numbers found.")
            .get(1)
            .expect("Did not properly capture port number.")
            .as_str()
            .parse()?;

        let states: Vec<bool> = STATE
            .captures(&self)
            .expect("No link states found.")
            .get(1)
            .expect("Did not properly capture link state.")
            .as_str()
            .split(',')
            .filter_map(|s| match s {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            })
            .collect();

        let link_statuses: Vec<u8> = LINK_STATUS
            .captures(&self)
            .expect("No link statuses found.")
            .get(1)
            .expect("Did not properly capture link statuses.")
            .as_str()
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();

        if link_statuses.len() < num_ports {
            return Err(anyhow!(
                "States size {} too small, expected at least {}. Issue with capturing.",
                states.len(),
                num_ports
            ));
        }

        let packet_counts: Vec<u128> = PACKETS
            .captures(&self)
            .expect("No packet lists found.")
            .get(1)
            .expect("Did not properly capture packet lists.")
            .as_str()
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();
        if packet_counts.len() < num_ports * 4 {
            return Err(anyhow!(format!(
                "Expected >= {} packet counts based on receiving {} states, but got {}.",
                states.len() * 4,
                states.len(),
                packet_counts.len()
            )));
        }

        let mut port_statistics = vec![];
        for index in 0..num_ports {
            let port_number: u8 = index.try_into().expect("Could not set index.");
            let port_number = port_number + 1;
            port_statistics.push(PortStatistic {
                port_number,
                enabled: *states.get(index).expect("Could not get state"),
                link_status: link_statuses
                    .get(index)
                    .expect("Could not get link status")
                    .to_owned()
                    .into(),
                tx_good_packets: packet_counts
                    .get(index * 4)
                    .expect("Could not get packet count")
                    .to_owned(),
                tx_bad_packets: packet_counts
                    .get(index * 4 + 1)
                    .expect("Could not get packet count")
                    .to_owned(),
                rx_good_packets: packet_counts
                    .get(index * 4 + 2)
                    .expect("Could not get packet count")
                    .to_owned(),
                rx_bad_packets: packet_counts
                    .get(index * 4 + 3)
                    .expect("Could not get packet count")
                    .to_owned(),
            })
        }

        Ok(TPLinkStats { port_statistics })
    }
}

impl LinkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkStatus::Down => "Link Down",
            LinkStatus::Auto => "Auto",
            LinkStatus::TenHalf => "10Half",
            LinkStatus::TenFull => "10Full",
            LinkStatus::OneHundredHalf => "100Half",
            LinkStatus::OneHundredFull => "100Full",
            LinkStatus::OneThousandFull => "1000Full",
            LinkStatus::Empty => "",
        }
    }

    pub fn as_int(&self) -> u16 {
        match self {
            LinkStatus::Down => 0,
            LinkStatus::Auto => 1,
            LinkStatus::TenHalf => 5,
            LinkStatus::TenFull => 10,
            LinkStatus::OneHundredHalf => 50,
            LinkStatus::OneHundredFull => 100,
            LinkStatus::OneThousandFull => 1000,
            LinkStatus::Empty => 0,
        }
    }

    pub fn is_connected(&self) -> bool {
        !matches!(
            self,
            LinkStatus::Down | LinkStatus::Auto | LinkStatus::Empty
        )
    }
}

impl From<u8> for LinkStatus {
    fn from(val: u8) -> Self {
        match val {
            0 => LinkStatus::Down,
            1 => LinkStatus::Auto,
            2 => LinkStatus::TenHalf,
            3 => LinkStatus::TenFull,
            4 => LinkStatus::OneHundredHalf,
            5 => LinkStatus::OneHundredFull,
            6 => LinkStatus::OneThousandFull,
            _ => LinkStatus::Empty,
        }
    }
}
