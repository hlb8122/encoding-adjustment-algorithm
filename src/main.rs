pub mod monitoring;
pub mod utils;

#[macro_use]
extern crate clap;

use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};
use clap::App;
use utils::decode_hex;
use monitoring::Monitor;
use influent::client::Credentials;

fn main() {
    // Setup CLI
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    // Setup RPC client
    let rpc_addr = matches
        .value_of("address")
        .unwrap_or("http://localhost:8332");
    let bitcoin_username = matches.value_of("busername").unwrap_or("");
    let bitcoin_password = matches.value_of("bpassword").unwrap_or("");
    let window = matches
        .value_of("window")
        .map(|s| s.parse::<usize>().unwrap_or(25))
        .unwrap_or(25);
    let level = 8;

    let rpc = Client::new(
        rpc_addr.to_string(),
        Auth::UserPass(bitcoin_username.to_string(), bitcoin_password.to_string()),
    )
    .unwrap();

    // Setup monitoring
    // let credentials = Credentials {
    //     database: ""
    // };
    // let monitor = Monitor::new(credentials)

    // Fetch all transactions in block window
    let mut block_hash = rpc.get_best_block_hash().unwrap();

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
    let dictionary = zstd::dict::from_samples(&raw_txs, 1024).unwrap();
    drop(raw_txs);

    // Compressors
    let mut compressor_nodict = zstd::block::Compressor::new();
    let mut compressor_dict = zstd::block::Compressor::with_dict(dictionary);

    // Begin compression loop
    let mut last_block_hash = rpc.get_best_block_hash().unwrap();

    loop {
        let current_block_hash = rpc.get_best_block_hash().unwrap();
        if current_block_hash == last_block_hash {
            // Sleep
            println!("no new block");
        } else {
            last_block_hash = current_block_hash;
            let block = rpc.get_block(&block_hash).unwrap();
            let raw_tx_inner: Vec<(String, Vec<u8>)> = block
                .txdata
                .iter()
                .map(|tx| (tx.txid().to_string(), decode_hex(&tx.raw_hex()).unwrap()))
                .collect();

            // Benchmark tx compression
            for (id, raw) in raw_tx_inner {
                let raw_slice = &raw[..];

                let out_wdict = compressor_dict.compress(raw_slice, level).unwrap();
                let out_wodict = compressor_nodict.compress(raw_slice, level).unwrap();

                println!("{} before compression", raw.len());
                println!("{} w dict", out_wdict.len());
                println!("{} wo. dict", out_wodict.len());
            }

            // Benchmark block compression
            let raw_block = rpc.get_block_hex(&block_hash);
        }
    }
}
