#![no_std]
#![no_main]

use bt_hci::{
    cmd::le::{
        LeAddDeviceToFilterAcceptList, LeClearFilterAcceptList, LeSetScanEnable, LeSetScanParams,
    },
    controller::ControllerCmdSync,
    param::LeAdvReport,
};

use embassy_futures::join::join;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{self, Channel},
};
use heapless::Vec;
use trouble_host::{
    Controller, Host, PacketPool,
    advertise::AdStructure,
    connection::{PhySet, ScanConfig},
    prelude::{BdAddr, EventHandler},
    scan::Scanner,
};

use common_types::TAG_NAME_MAX_LEN;

use embassy_time::{Duration, Instant, Timer};

pub const CHANNEL_CAPACITY: usize = 32;

#[derive(Clone, Debug)]
pub struct TagReport {
    pub addr: BdAddr,
    pub name: heapless::String<TAG_NAME_MAX_LEN>,
    pub timestamp: embassy_time::Instant,
    pub rssi: i8,
    pub sequence: u32,
}

pub struct TagScanner {
    queue: Channel<CriticalSectionRawMutex, TagReport, CHANNEL_CAPACITY>,

    prefix: [u8; 3],
}

impl TagScanner {
    pub const fn new(prefix: [u8; 3]) -> Self {
        Self {
            prefix,
            queue: Channel::new(),
        }
    }

    pub async fn run<'stack, C, P: PacketPool>(
        &self,
        mut host: Host<'stack, C, P>,
        interval: Duration,
        window: Duration,
    ) where
        C: Controller
            + ControllerCmdSync<LeSetScanParams>
            + ControllerCmdSync<LeSetScanEnable>
            + ControllerCmdSync<LeClearFilterAcceptList>
            + ControllerCmdSync<LeAddDeviceToFilterAcceptList>,
    {
        let event_handler = ScanEventHandler::new(self.prefix, self.queue.sender());

        let mut scanner = Scanner::new(host.central);
        let _ = join(host.runner.run_with_handler(&event_handler), async {
            let config = ScanConfig::<'_> {
                active: true,
                phys: PhySet::M1,

                // There's an issue with the Duration https://github.com/embassy-rs/bt-hci/pull/74
                // Workaround is to multiply the value by 16.

                // Max scan interval in the BLE spec is 10s.
                interval,
                // Beacon advertising frequency is between 1Hz and 10Hz, staying up makes sure we can catch at least one advertisement.
                window,
                ..Default::default()
            };
            let mut _session = scanner.scan(&config).await.unwrap();
            // Scan forever
            loop {
                Timer::after_secs(1000).await;
            }
        })
        .await;
    }

    pub fn receiver(
        &self,
    ) -> channel::Receiver<'_, CriticalSectionRawMutex, TagReport, CHANNEL_CAPACITY> {
        self.queue.receiver()
    }
}

struct ScanEventHandler<'a> {
    prefix: [u8; 3],
    sender: channel::Sender<'a, CriticalSectionRawMutex, TagReport, CHANNEL_CAPACITY>,
}

impl<'a> ScanEventHandler<'a> {
    pub fn new(
        prefix: [u8; 3],
        sender: channel::Sender<'a, CriticalSectionRawMutex, TagReport, CHANNEL_CAPACITY>,
    ) -> Self {
        Self { prefix, sender }
    }
}

impl ScanEventHandler<'_> {
    /// Returns name and sequence number from the advertisement.
    fn parse_manufacturer_data(
        report: LeAdvReport,
    ) -> Option<(heapless::String<TAG_NAME_MAX_LEN>, u32)> {
        let adv_data = AdStructure::decode(report.data);

        let mut sequence = None;
        let mut name: Option<heapless::String<TAG_NAME_MAX_LEN>> = None;
        for adv in adv_data {
            match adv {
                Ok(AdStructure::ManufacturerSpecificData {
                    company_identifier,
                    payload,
                }) => {
                    if company_identifier != 0xffff {
                        return None;
                    }

                    if payload.len() < 4 {
                        return None;
                    }

                    let mut sequence_data: [u8; 4] = [0; 4];
                    sequence_data.copy_from_slice(payload.get(0..=3)?);

                    sequence = Some(u32::from_be_bytes(sequence_data));
                }
                Ok(AdStructure::CompleteLocalName(data)) => {
                    let container: Vec<u8, TAG_NAME_MAX_LEN> = Vec::from_slice(data).ok()?;
                    name = heapless::String::from_utf8(container).ok();
                }

                Ok(_) => {}
                Err(_) => {}
            }
        }

        name.and_then(|name| sequence.map(|sequence| (name, sequence)))
    }
}

impl EventHandler for ScanEventHandler<'_> {
    fn on_adv_reports(&self, mut it: bt_hci::param::LeAdvReportsIter) {
        let instant = Instant::now();

        while let Some(Ok(report)) = it.next() {
            let addr = report.addr;
            let rssi = report.rssi;

            // Prefix in big endian is suffix here
            if !addr.into_inner().ends_with(&self.prefix) {
                continue;
            }

            if let Some((name, sequence)) = Self::parse_manufacturer_data(report) {
                let _ = self.sender.try_send(TagReport {
                    addr,
                    name,
                    timestamp: instant,
                    rssi,
                    sequence,
                });
            }
        }
    }
}
