use core::cell::RefCell;

use coap_handler::Attribute;
use coap_handler_implementations::{GetRenderable, TypeHandler, wkc::ConstantSingleRecordReport};
use coap_request::Stack;
use common_types::GatewayUpdate;
use embassy_sync::{
    blocking_mutex::{self, raw::CriticalSectionRawMutex},
    signal::Signal,
};

use ariel_os::{log::info, time::Timer};

use crate::config::COAP_ENDPOINT;

static LAST_UPDATE: blocking_mutex::Mutex<CriticalSectionRawMutex, RefCell<Option<GatewayUpdate>>> =
    blocking_mutex::Mutex::new(RefCell::new(None));
static UPDATE_READ: Signal<CriticalSectionRawMutex, bool> = Signal::new();

pub async fn send_update(update: GatewayUpdate) {
    LAST_UPDATE.lock(|s| s.borrow_mut().replace(update));
    UPDATE_READ.reset();

    // 30s Timeout
    embassy_futures::select::select(register_to_rd(), Timer::after_secs(30)).await;

    embassy_futures::select::select(UPDATE_READ.wait(), Timer::after_secs(30)).await;

    // Wait 2 secs for transfer to finish
    Timer::after_secs(2).await;
}

async fn register_to_rd() {
    let client = ariel_os::coap::coap_client().await;

    let demoserver = COAP_ENDPOINT.parse().unwrap();

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
}

#[ariel_os::task(autostart)]
async fn coap_run() {
    use coap_handler_implementations::{HandlerBuilder, SimpleRendered, new_dispatcher};

    let handler = new_dispatcher()
        // test route
        .at(&["hello"], SimpleRendered("Hello from Ariel OS"))
        // the route that returns the status
        .at(
            &["status"],
            ConstantSingleRecordReport::new(
                TypeHandler::new_minicbor_2(coap_handler_implementations::with_get(
                    StatusRenderer::new(),
                )),
                &[Attribute::Title("Gateway Status")],
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

        UPDATE_READ.signal(true);

        LAST_UPDATE
            .lock(|s| s.clone())
            .into_inner()
            .ok_or(coap_message_utils::Error::service_unavailable())
    }
}
