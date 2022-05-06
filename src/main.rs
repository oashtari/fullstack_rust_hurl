use structopt::StructOpt;
use heck::TitleCase;
use log::trace;

mod app;
mod client;
mod errors;

use errors::HurlResult;

fn main() {
    println!("Hello, world!");
}
