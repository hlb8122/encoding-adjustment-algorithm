pub mod monitoring;
pub mod utils;

#[macro_use]
extern crate clap;

use std::{
    env,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use bitcoin::{
    consensus::encode::Decodable, util::hash::BitcoinHash, util::psbt::serialize::Deserialize,
    Block, Transaction,
};
use bitcoin_hashes::hex::ToHex;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use clap::App;
use futures::{future, future::ok, Future, Stream};
use futures_zmq::{prelude::*, Sub};
use influent::client::Credentials;
use log::{error, info};

use monitoring::Monitor;
use utils::ObjectType;

const DEFAULT_TRAINING_WINDOW: usize = 1024;
const DEFAULT_RESET_PERIOD: usize = 16;
const DEFAULT_BITCOIN_IP: &str = "localhost";
const DEFAULT_BITCOIN_USERNAME: &str = "";
const DEFAULT_BITCOIN_PASSWORD: &str = "";
const DEFAULT_INFLUX_ADDRESS: &str = "http://localhost:8332";
const DEFAULT_INFLUX_USERNAME: &str = "";
const DEFAULT_INFLUX_PASSWORD: &str = "";
const DEFAULT_COMPRESSION_LEVEL: i32 = 22;
const DEFAULT_DICT_SIZE: usize = 8;

fn main() {
    // Setup CLI
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(&yaml).get_matches().clone();

    let bitcoin_ip = matches
        .value_of("bitcoin-ip")
        .unwrap_or(DEFAULT_BITCOIN_IP);
    let bitcoin_username = matches
        .value_of("bitcoin-username")
        .unwrap_or(DEFAULT_BITCOIN_USERNAME);
    let bitcoin_password = matches
        .value_of("bitcoin-password")
        .unwrap_or(DEFAULT_BITCOIN_PASSWORD);

    // Setup RPC client
    let rpc = Client::new(
        format!("http://{}:8332", bitcoin_ip),
        Auth::UserPass(bitcoin_username.to_string(), bitcoin_password.to_string()),
    )
    .unwrap();

    // Setup ZeroMQ
    let context = Arc::new(zmq::Context::new());
    let sub = Sub::builder(context.clone())
        .connect(&format!("tcp://{}:28332", bitcoin_ip))
        .filter(b"")
        .build();

    // Setup logging
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Setup monitoring
    let influx_addr = matches
        .value_of("influx-address")
        .unwrap_or(DEFAULT_INFLUX_ADDRESS);
    let influx_username = matches
        .value_of("influx-username")
        .unwrap_or(DEFAULT_INFLUX_USERNAME);
    let influx_password = matches
        .value_of("influx-password")
        .unwrap_or(DEFAULT_INFLUX_PASSWORD);

    let credentials = Credentials {
        database: "compression",
        username: influx_username,
        password: influx_password,
    };
    let monitor = Monitor::new(credentials, influx_addr);

    // Setup training window
    let window = matches
        .value_of("window")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_TRAINING_WINDOW))
        .unwrap_or(DEFAULT_TRAINING_WINDOW);
    let reset_period = matches
        .value_of("reset-period")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_RESET_PERIOD))
        .unwrap_or(DEFAULT_TRAINING_WINDOW);

    let block_hash = rpc.get_best_block_hash().unwrap();
    let training_set = utils::fetch_training_data(&rpc, block_hash, window);

    // Setup dictionary generation
    let dictionary_size = matches
        .value_of("dictionary-size")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_DICT_SIZE))
        .unwrap_or(DEFAULT_DICT_SIZE)
        * 1024;

    let dictionary = utils::train_dictionary(training_set, dictionary_size);

    // Compressors
    let level = matches
        .value_of("compression-level")
        .map(|s| s.parse::<i32>().unwrap_or(DEFAULT_COMPRESSION_LEVEL))
        .unwrap_or(DEFAULT_COMPRESSION_LEVEL);
    let mut compressor_nodict = zstd::block::Compressor::new();
    let mut compressor_dict = zstd::block::Compressor::with_dict(dictionary);

    // Begin compression loop
    info!("starting compression loop...");
    let block_counter = Arc::new(AtomicUsize::new(0));

    let runner = sub
        .map_err(|e| {
            error!("zmq subscriptions error = {}", e);
        })
        .and_then(move |sub| {
            // For each message received via ZMQ
            sub.stream()
                .map_err(|e| {
                    error!("zmq stream error = {}", e);
                })
                .for_each(move |multipart| {
                    match &**multipart.get(0).unwrap() {
                        b"rawtx" => {
                            // Decode
                            let raw_tx: &[u8] = &multipart.get(1).unwrap();
                            let new_tx = Transaction::deserialize(raw_tx).unwrap();
                            let id = new_tx.txid().to_hex();
                            info!("benchmarking on tx {}", id);

                            // Compress
                            let out_wdict = compressor_dict.compress(raw_tx, level).unwrap();
                            let out_wodict = compressor_nodict.compress(raw_tx, level).unwrap();

                            let raw_size = raw_tx.len();
                            let comp_wo_dict_size = out_wodict.len();
                            let comp_w_dict_size = out_wdict.len();
                            info!("raw size: {} bytes", raw_size);
                            info!("compressed w/o dict size: {} bytes", comp_wo_dict_size);
                            info!("compressed w dict size: {} bytes", comp_w_dict_size);

                            let ratio_raw_to_wo = comp_wo_dict_size as f32 / raw_size as f32;
                            let ratio_raw_to_w = comp_w_dict_size as f32 / raw_size as f32;

                            info!("ratios: 1 | {} | {}", ratio_raw_to_wo, ratio_raw_to_w);

                            // Write to influxdb
                            monitor.write(
                                &id,
                                ObjectType::Transaction,
                                raw_size,
                                comp_wo_dict_size,
                                comp_w_dict_size,
                            );

                            future::Either::A(ok(()))
                        }
                        b"rawblock" => {
                            // Decode
                            block_counter.fetch_add(1, Ordering::SeqCst);
                            let mut raw_block: &[u8] = &multipart.get(1).unwrap();
                            let block = Block::consensus_decode(&mut raw_block).unwrap();
                            let id = block.bitcoin_hash();

                            info!("benchmarking on block {}", id);

                            // Retrain
                            if block_counter.load(Ordering::SeqCst) % reset_period == 0 {
                                let training_set = utils::fetch_training_data(&rpc, id, window);
                                let dictionary =
                                    utils::train_dictionary(training_set, dictionary_size);
                                compressor_nodict = zstd::block::Compressor::new();
                                compressor_dict = zstd::block::Compressor::with_dict(dictionary);
                            }

                            // Compress
                            let out_wdict = compressor_dict.compress(&raw_block, level).unwrap();
                            let out_wodict = compressor_nodict.compress(&raw_block, level).unwrap();

                            let raw_size = raw_block.len();
                            let comp_wo_dict_size = out_wodict.len();
                            let comp_w_dict_size = out_wdict.len();

                            info!("raw size: {} bytes", raw_size);
                            info!("compressed w/o dict size: {} bytes", comp_wo_dict_size);
                            info!("compressed w dict size: {} bytes", comp_w_dict_size);

                            let ratio_raw_to_wo = comp_wo_dict_size as f32 / raw_size as f32;
                            let ratio_raw_to_w = comp_w_dict_size as f32 / raw_size as f32;

                            info!(
                                "compression ratio: 1 | {} | {}",
                                ratio_raw_to_wo, ratio_raw_to_w
                            );

                            // Write to influxdb
                            monitor.write(
                                &id.to_hex(),
                                ObjectType::Block,
                                raw_size,
                                comp_wo_dict_size,
                                comp_w_dict_size,
                            );
                            future::Either::B(ok(()))
                        }
                        _ => {
                            error!("unexpected zmq message");
                            unreachable!()
                        }
                    }
                })
        })
        .map(|_| ());

    // Run loop
    let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
    rt.block_on(runner).unwrap();
}
