pub mod monitoring;
pub mod utils;

#[macro_use]
extern crate clap;

use std::thread::sleep;
use std::time::Duration;

use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};
use clap::App;
use influent::client::Credentials;
use log::info;
use std::env;

use monitoring::Monitor;
use utils::{decode_hex, ObjectType};

const BLOCK_CHECK_PERIOD: Duration = Duration::from_millis(1_000);
const DEFAULT_TRAINING_WINDOW: usize = 1024;
const DEFAULT_RESET_PERIOD: usize = 16;
const DEFAULT_BITCOIN_ADDRESS: &str = "http://localhost:8332";
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
    let matches = App::from_yaml(yaml).get_matches();

    // Setup RPC client
    let rpc_addr = matches
        .value_of("bitcoin-address")
        .unwrap_or(DEFAULT_BITCOIN_ADDRESS);
    let bitcoin_username = matches
        .value_of("bitcoin-username")
        .unwrap_or(DEFAULT_BITCOIN_USERNAME);
    let bitcoin_password = matches
        .value_of("bitcoin-password")
        .unwrap_or(DEFAULT_BITCOIN_PASSWORD);

    let rpc = Client::new(
        rpc_addr.to_string(),
        Auth::UserPass(bitcoin_username.to_string(), bitcoin_password.to_string()),
    )
    .unwrap();

    // Setup logging
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Setup monitoring
    let influx_addr = matches
        .value_of("influx-address")
        .unwrap_or(DEFAULT_INFLUX_ADDRESS);
    let influx_username = matches.value_of("influx-username").unwrap_or(DEFAULT_INFLUX_USERNAME);
    let influx_password = matches.value_of("influx-password").unwrap_or(DEFAULT_INFLUX_PASSWORD);

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
    let training_set = utils::fetch_training_data(&rpc, block_hash, &window);

    // Setup dictionary generation
    let dictionary_size = matches
        .value_of("dictionary-size")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_DICT_SIZE))
        .unwrap_or(DEFAULT_DICT_SIZE) * 1024;

    let dictionary = utils::train_dictionary(training_set, &dictionary_size);

    // Compressors
    let level = matches
        .value_of("compression-level")
        .map(|s| s.parse::<i32>().unwrap_or(DEFAULT_COMPRESSION_LEVEL))
        .unwrap_or(DEFAULT_COMPRESSION_LEVEL);
    let mut compressor_nodict = zstd::block::Compressor::new();
    let mut compressor_dict = zstd::block::Compressor::with_dict(dictionary);

    // Begin compression loop
    info!("starting compression loop...");
    let mut last_block_hash: bitcoin_hashes::sha256d::Hash = Default::default();
    let mut block_counter = 0;

    loop {
        let current_block_hash = rpc.get_best_block_hash().unwrap();

        if current_block_hash == last_block_hash {
            // Sleep
            info!("waiting for new block...");
            sleep(BLOCK_CHECK_PERIOD);
        } else {
            info!("new block found; running compression");
            last_block_hash = current_block_hash;

            // Retrain
            block_counter += 1;
            if block_counter % reset_period == 0 {
                let training_set = utils::fetch_training_data(&rpc, current_block_hash, &window);
                let dictionary = utils::train_dictionary(training_set, &dictionary_size);
                compressor_nodict = zstd::block::Compressor::new();
                compressor_dict = zstd::block::Compressor::with_dict(dictionary);
            }

            let block = rpc.get_block(&current_block_hash).unwrap();
            let raw_tx_inner: Vec<(String, Vec<u8>)> = block
                .txdata
                .iter()
                .map(|tx| (tx.txid().to_string(), decode_hex(&tx.raw_hex())))
                .collect();

            // Benchmark tx compression
            for (id, raw) in raw_tx_inner {
                info!("benchmarking on tx {}", id);
                let raw_tx = &raw;

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

                monitor.write(&id, ObjectType::Transaction, raw_size, comp_wo_dict_size, comp_w_dict_size);
            }

            // Benchmark block compression
            info!("benchmarking on block {}", last_block_hash);
            let raw_block = decode_hex(&rpc.get_block_hex(&block_hash).unwrap());

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

            info!("compression ratio: 1 | {} | {}", ratio_raw_to_wo, ratio_raw_to_w);

            let id = current_block_hash.to_string();
            monitor.write(&id, ObjectType::Block, raw_size, comp_wo_dict_size, comp_w_dict_size);
        }
    }
}
