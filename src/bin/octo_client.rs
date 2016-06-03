extern crate octobuild;
extern crate hyper;
extern crate rustc_serialize;
extern crate rand;
extern crate tempdir;
#[macro_use]
extern crate log;

use octobuild::vs::compiler::VsCompiler;
use octobuild::io::statistic::Statistic;
use octobuild::compiler::*;
use octobuild::cache::Cache;
use octobuild::config::Config;
use octobuild::cluster::common::{BuilderInfo, RPC_BUILDER_LIST};

use tempdir::TempDir;
use hyper::{Client, Url};
use rustc_serialize::json;

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io;
use std::io::{Read, Write};
use std::iter::FromIterator;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::net::{SocketAddr, TcpStream};
use std::process;

fn main() {
    octobuild::utils::init_logger();

    let client = Client::new();
    match client
    .get(Url::parse("http://localhost:3000").unwrap().join(RPC_BUILDER_LIST).unwrap())
    .send()
    {
        Ok(mut response) => {
            let mut payload = String::new();
            response.read_to_string(&mut payload).unwrap();

            let builders: Vec<BuilderInfo> = json::decode(&payload).unwrap();
            let builder = get_random_builder(&builders).unwrap();
            info!("Builder: {} {}", builder.name, builder.endpoint);
            let addr = SocketAddr::from_str(&builder.endpoint).unwrap();

            let mut stream = TcpStream::connect(addr).unwrap();
            let mut payload = String::new();
            stream.read_to_string(&mut payload).unwrap();
            info!("{}", payload);
        }
        Err(e) => {
            info!("Builder: can't send info to coordinator: {}", e.description());
        }
    };
}

fn get_random_builder(builders: &Vec<BuilderInfo>) -> Option<&BuilderInfo> {
    if builders.len() > 0 {
        Some(&builders[rand::random::<usize>() % builders.len()])
    } else {
        None
    }
}