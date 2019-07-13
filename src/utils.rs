use bitcoin::{util::psbt::serialize::Serialize, Transaction};
use bitcoin_hashes::sha256d::Hash;
use bitcoincore_rpc::{Client, RpcApi};
use log::info;

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

pub fn fetch_training_data(rpc: &Client, tip: Hash, window: usize) -> Vec<Vec<u8>> {
    // Fetch all transactions in training window
    info!("fetching training window...");
    let mut raw_txs = Vec::with_capacity(window * 1024);
    let mut block_hash = tip;

    for i in 0..window {
        info!(
            "({} of {}) adding block {} to the training set",
            i, window, block_hash
        );
        let block = rpc.get_block(&block_hash).unwrap();

        let mut raw_tx_inner: Vec<Vec<u8>> = block
            .txdata
            .iter()
            .map(|tx| Transaction::serialize(&tx))
            .collect();
        raw_txs.append(&mut raw_tx_inner);

        block_hash = block.header.prev_blockhash;
    }
    raw_txs
}

pub fn train_dictionary(raw_txs: Vec<Vec<u8>>, dictionary_size: usize) -> Vec<u8> {
    // Train on block window
    info!("training dictionary...");
    zstd::dict::from_samples(&raw_txs, 1024 * dictionary_size).unwrap()
}
