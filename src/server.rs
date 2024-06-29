use escpos::{driver::UsbDriver};
use warp::Filter;
use serde_json::json;

use crate::print::handle_test_print;

pub async fn run( driver: UsbDriver) {
    let routes = routes(driver);
    println!("Serving the server!");
    warp::serve(routes).run(([127, 0, 0, 1], 55000)).await;
    
}

pub fn routes( driver: UsbDriver) -> impl Filter<Extract =  impl warp::Reply, Error = warp::Rejection> + Clone {
    hello_route().or(print_route(driver))
}

fn hello_route() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path::end().map(|| warp::reply::json(&json!("Hello Word!")))
}

fn print_route(driver: UsbDriver) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("print")
        .and(warp::post())
        .and(with_driver(driver.clone()))
        .map(|driver: UsbDriver| {
            let _ = handle_test_print(driver);
            warp::reply::json(&json!("Print request received and processed!"))
        })
}

fn with_driver(
    driver: UsbDriver,
) -> impl Filter<Extract = (UsbDriver,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || driver.clone())
}