use print::initialize_device;
use crate::server::run;

mod server;
mod print;
mod models;


#[tokio::main(flavor="current_thread")]
async fn main() {
    let device =  initialize_device().await;

    run(device).await;
}
