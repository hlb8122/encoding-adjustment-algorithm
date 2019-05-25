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

use utils::{decode_hex, ObjectType, CompressionType};
use monitoring::Monitor;

const DEFAULT_WINDOW: usize = 5;

fn main() {
    // Setup CLI
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    // Setup RPC client
    let rpc_addr = matches
        .value_of("bitcoin-address")
        .unwrap_or("http://localhost:8332");
    let bitcoin_username = matches.value_of("bitcoin-username").unwrap_or("");
    let bitcoin_password = matches.value_of("bitcoin-password").unwrap_or("");
    let window = matches
        .value_of("window")
        .map(|s| s.parse::<usize>().unwrap_or(DEFAULT_WINDOW))
        .unwrap_or(DEFAULT_WINDOW);
    let level = 8;

    let rpc = Client::new(
        rpc_addr.to_string(),
        Auth::UserPass(bitcoin_username.to_string(), bitcoin_password.to_string()),
    )
    .unwrap();

    // Setup logging
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Setup monitoring
    let credentials = Credentials {
        database: "compression",
        username: "",
        password: ""
    };
    let monitor = Monitor::new(credentials, "http://35.202.119.18:8086");

    // Fetch all transactions in training window
    info!("fetching training window...");

    let mut block_hash = rpc.get_best_block_hash().unwrap();
    info!("current chaintip {}", block_hash);

    let mut raw_txs = Vec::with_capacity(window * 1024);
    for _ in 0..window {
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
    let dictionary = zstd::dict::from_samples(&raw_txs, 1024).unwrap();
    drop(raw_txs);

    // Compressors
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
            sleep_ms(5_000);
            
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
                let raw_slice = &raw[..];

                let out_wdict = compressor_dict.compress(raw_slice, level).unwrap();
                let out_wodict = compressor_nodict.compress(raw_slice, level).unwrap();

                let uncomp_size = raw.len();
                let comp_wo_dict_size = out_wodict.len();
                let comp_w_dict_size = out_wdict.len();
                info!("raw size: {} bytes", uncomp_size);
                info!("compressed w/o dict size: {} bytes", uncomp_size);
                info!("compressed w dict size: {} bytes", comp_w_dict_size);

                monitor.write(&id, ObjectType::Transaction, None, uncomp_size).poll().unwrap();
                monitor.write(&id, ObjectType::Transaction, Some(CompressionType::NoDict), comp_wo_dict_size).poll().unwrap();
                monitor.write(&id, ObjectType::Transaction, Some(CompressionType::Dict), comp_w_dict_size).poll().unwrap();
            }

            // Benchmark block compression
            // let raw_block = rpc.get_block_hex(&block_hash);
        }
    }
}
