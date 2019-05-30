# Encoding Adjustment Algorithm Benchmarker

Uses the last `window` blocks to train a dictionary. Grabs each new block and benchmarks raw size/compression without dictionary/compression with dictionary for each raw transaction. Records the measurements to an influxdb.



## Build

```bash
sudo apt install clang
cargo build --release
```

## Command Line Interface

```yaml
- bitcoin-address:
    long: bitcoin-address
    help: Sets the address of the Bitcoin RPC
    takes_value: true
- bitcoin-username:
    long: bitcoin-username
    help: Sets the username for the Bitcoin RPC
    takes_value: true
- bitcoin-password:
    long: bitcoin-password
    help: Sets the password for the Bitcoin RPC
    takes_value: true
- influx-address:
    long: influx-address
    help: Sets the address of the InfluxDB instance
    takes_value: true
- influx-username:
    long: influx-username
    help: Sets the username for the InfluxDB instance
    takes_value: true
- influx-password:
    long: influx-password
    help: Sets the password for the InfluxDB instance
    takes_value: true
- window:
    short: w
    long: window
    help: Sets the number of blocks in the training window (number of blocks)
    takes_value: true
- reset-period:
    short: r
    long: reset-period
    help: Sets the frequency of retraining (number of blocks)
    takes_value: true
- compression-level:
    short: l
    long: compression-level
    help: Sets the level of compression (0-22)
    takes_value: true
- dictionary-size:
    short: d
    long: dictionary-size
    help: Sets the maximum dictionary size (kilobytes)
    takes_value: true
```
