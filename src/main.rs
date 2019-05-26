pub mod monitoring;
pub mod utils;

#[macro_use]
extern crate clap;

use std::thread::sleep_ms;

use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};
use clap::App;
use influent::client::Credentials;
use log::info;
use std::env;

use monitoring::Monitor;
use utils::{decode_hex, CompressionType, ObjectType};

const DEFAULT_TRAINING_WINDOW: usize = 512;
const DEFAULT_BITCOIN_ADDRESS: &str = "http://localhost:8332";
const DEFAULT_BITCOIN_USERNAME: &str = "";
const DEFAULT_BITCOIN_PASSWORD: &str = "";
const DEFAULT_INFLUX_ADDRESS: &str = "http://localhost:8332";
const DEFAULT_INFLUX_USERNAME: &str = "";
const DEFAULT_INFLUX_PASSWORD: &str = "";
const DEFAULT_COMPRESSION_LEVEL: i32 = 22;
const DEFAULT_DICT_SIZE: usize = 1024 * 8;

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

    // Fetch all transactions in training window
    info!("fetching training window...");
    let window = matches
        .value_of("window")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_TRAINING_WINDOW))
        .unwrap_or(DEFAULT_TRAINING_WINDOW);
    let mut block_hash = rpc.get_best_block_hash().unwrap();
    let mut raw_txs = Vec::with_capacity(window * 1024);

    for i in 0..window {
        info!(
            "({} of window) adding block {} to the training set",
            i, block_hash
        );
        let block = rpc.get_block(&block_hash).unwrap();

        let mut raw_tx_inner: Vec<Vec<u8>> = block
            .txdata
            .iter()
            .map(|tx| decode_hex(&tx.raw_hex()).unwrap())
            .collect();
        raw_txs.append(&mut raw_tx_inner);

        block_hash = block.header.prev_blockhash;
    }

    // Train on block window
    info!("training dictionary...");
    let dictionary_size = matches
        .value_of("dictionary-size")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_DICT_SIZE))
        .unwrap_or(DEFAULT_DICT_SIZE);
    let dictionary = zstd::dict::from_samples(&raw_txs, 1024 * dictionary_size).unwrap();
    drop(raw_txs);

    // Compressors
    let level = matches
        .value_of("compression-level")
        .map(|s| s.parse::<i32>().unwrap_or(DEFAULT_COMPRESSION_LEVEL))
        .unwrap_or(DEFAULT_COMPRESSION_LEVEL);;
    let mut compressor_nodict = zstd::block::Compressor::new();
    let mut compressor_dict = zstd::block::Compressor::with_dict(dictionary);

    // Begin compression loop
    info!("starting training loop...");
    let mut last_block_hash: bitcoin_hashes::sha256d::Hash = Default::default();

    loop {
        let current_block_hash = rpc.get_best_block_hash().unwrap();
        if current_block_hash == last_block_hash {
            // Sleep
            info!("waiting for new block...");
            sleep_ms(10_000);
        } else {
            info!("new block found; running compression");
            last_block_hash = current_block_hash;
            let block = rpc.get_block(&block_hash).unwrap();
            let raw_tx_inner: Vec<(String, Vec<u8>)> = block
                .txdata
                .iter()
                .map(|tx| (tx.txid().to_string(), decode_hex(&tx.raw_hex()).unwrap()))
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

                info!("ratios: 1 - {} - {}", ratio_raw_to_wo, ratio_raw_to_w);

                monitor.write(&id, ObjectType::Transaction, None, raw_size);
                monitor.write(
                    &id,
                    ObjectType::Transaction,
                    Some(CompressionType::NoDict),
                    comp_wo_dict_size,
                );
                monitor.write(
                    &id,
                    ObjectType::Transaction,
                    Some(CompressionType::Dict),
                    comp_w_dict_size,
                );
            }

            // Benchmark block compression
            info!("benchmarking on block {}", last_block_hash);
            let raw_block = decode_hex(&rpc.get_block_hex(&block_hash).unwrap()).unwrap();

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

            info!("ratios: 1 - {} - {}", ratio_raw_to_wo, ratio_raw_to_w);

            let id = last_block_hash.to_string();
            monitor.write(&id, ObjectType::Block, None, raw_size);
            monitor.write(
                &id,
                ObjectType::Block,
                Some(CompressionType::NoDict),
                comp_wo_dict_size,
            );
            monitor.write(
                &id,
                ObjectType::Block,
                Some(CompressionType::Dict),
                comp_w_dict_size,
            );
        }
    }
}
