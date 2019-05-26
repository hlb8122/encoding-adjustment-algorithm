use std::num::ParseIntError;

use log::info;
use bitcoincore_rpc::{Client, RpcApi, RawTx};
use bitcoin_hashes::sha256d::Hash;

pub enum ObjectType {
    Block,
    Transaction,
}

impl Into<&str> for ObjectType {
    fn into(self) -> &'static str {
        match self {
            ObjectType::Block => "block",
            ObjectType::Transaction => "transaction",
        }
    }
}

pub fn decode_hex(s: &str) -> Vec<u8> {
    let res: Result<Vec<u8>, ParseIntError> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();
    res.unwrap()
}

pub fn fetch_training_data(rpc: &Client, tip: Hash, window: &usize) -> Vec<Vec<u8>> {
    // Fetch all transactions in training window
    info!("fetching training window...");
    let mut raw_txs = Vec::with_capacity(window * 1024);
    let mut block_hash = tip;

    for i in 0..*window {
        info!(
            "({} of {}) adding block {} to the training set",
            i, window, block_hash
        );
        let block = rpc.get_block(&block_hash).unwrap();

        let mut raw_tx_inner: Vec<Vec<u8>> = block
            .txdata
            .iter()
            .map(|tx| decode_hex(&tx.raw_hex()))
            .collect();
        raw_txs.append(&mut raw_tx_inner);

        block_hash = block.header.prev_blockhash;
    }
    raw_txs
}

pub fn train_dictionary(raw_txs: Vec<Vec<u8>>, dictionary_size: &usize) -> Vec<u8> {
    // Train on block window
    info!("training dictionary...");
    zstd::dict::from_samples(&raw_txs, 1024 * dictionary_size).unwrap()
}