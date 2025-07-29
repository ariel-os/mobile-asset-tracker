#![no_main]
#![no_std]
use core::{cell::RefCell, str::FromStr};

use coap_handler::Attribute;
use coap_handler_implementations::{GetRenderable, TypeHandler, wkc::ConstantSingleRecordReport};
use coap_request::Stack;
use embassy_sync::{
    blocking_mutex::{self, raw::CriticalSectionRawMutex},
    mutex::Mutex,
};
use embedded_io_async::BufRead;
use heapless::{String, Vec, index_map::FnvIndexMap};

use ariel_os::{
    asynch::Spawner,
    debug::log::{Debug2Format, debug, error, info, warn},
    gpio::{Input, Level, Output, Pull},
    hal,
    time::{Duration, Instant, Timer},
};

use common_types::{AddressesSeen, DetectedTag, GatewayUpdate, Location, MAX_SEEN};


static LAST_UPDATE: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<Option<GatewayUpdate>>> =
    blocking_mutex::Mutex::new(RefCell::new(None));


#[ariel_os::task(autostart)]
async fn register_to_rd() {
    let client = ariel_os::coap::coap_client().await;

    // Corresponding to the fixed network setup, we select a fixed server address; this may need to
    // be updated on hosts that are configured differently.
    let addr = "192.168.1.16:4230"; // IPv4 🔔
    let demoserver = addr.parse().unwrap();

    loop {
        info!("Sending POST to {}...", demoserver);
        let request = coap_request_implementations::Code::post()
            .with_path("/rd")
            .with_request_payload_slice(b"This is Ariel OS")
            .processing_response_payload_through(|p| {
                info!(
                    "RD response is {:?}",
                    core::str::from_utf8(p).map_err(|_| "not Unicode?")
                );
            });
        let response = client.to(demoserver).request(request).await;
        info!("Response {:?}", response.map_err(|_| "TransportError"));

        Timer::after_secs(10).await
    }
}

#[ariel_os::task(autostart)]
async fn coap_run() {
    use coap_handler_implementations::{HandlerBuilder, SimpleRendered, new_dispatcher};

    let handler = new_dispatcher()
        // We offer a single resource: /hello, which responds just with a text string.
        .at(&["hello"], SimpleRendered("Hello from Ariel OS"))
        .at(
            &["status"],
            ConstantSingleRecordReport::new(
                TypeHandler::new_minicbor_2(coap_handler_implementations::with_get(
                    StatusRenderer::new(),
                )),
                &[Attribute::Observable, Attribute::Title("Gateway Status")],
            ),
        );

    ariel_os::coap::coap_run(handler).await;
}

struct StatusRenderer {}

impl StatusRenderer {
    pub fn new() -> StatusRenderer {
        StatusRenderer {}
    }
}

impl GetRenderable for StatusRenderer {
    type Get = GatewayUpdate;
    fn get(&mut self) -> Result<Self::Get, coap_message_utils::Error> {
        info!("GET /status");

        LAST_UPDATE
            .lock(|s| s.clone())
            .into_inner()
            .ok_or_else(|| coap_message_utils::Error::service_unavailable())
    }
}
