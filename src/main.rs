pub mod monitoring;

#[macro_use]
extern crate clap;

use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};
use clap::App;

fn main() {
    // Setup CLI
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    // Setup RPC client
    let rpc_addr = matches
        .value_of("address")
        .unwrap_or("http://localhost:8332");
    let username = matches.value_of("username").unwrap_or("");
    let password = matches.value_of("password").unwrap_or("");
    let window = matches
        .value_of("window")
        .map(|s| s.parse::<usize>().unwrap_or(25))
        .unwrap_or(25);

    let rpc = Client::new(
        rpc_addr.to_string(),
        Auth::UserPass(username.to_string(), password.to_string()),
    )
    .unwrap_or(panic!("couldn't construct RPC client"));

    // Fetch all transactions in block window
    let mut block_hash = rpc
        .get_best_block_hash()
        .unwrap_or(panic!("couldn't get tip"));

    let mut raw_txs = Vec::with_capacity(window * 1024);
    for _ in 0..window {
        let block = rpc
            .get_block(&block_hash)
            .unwrap_or(panic!("couldn't get block"));

        let raw_tx_inner: Vec<String> = block.txdata.iter().map(|tx| tx.raw_hex()).collect();
        raw_txs.append(&mut raw_tx_inner);

        block_hash = block.header.prev_blockhash;
    }


}
