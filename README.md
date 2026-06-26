# Paqus Full Node

Rust full node for the Paqus testnet. It handles local chain storage, mining,
RPC, peer sync, gateway-based peer discovery, and manual bootstrap peers.

## Storage

The node stores chain data in LMDB. Use a directory as the database path:

```text
./data/paqus/
  data.mdb
  lock.mdb
```

`data.mdb` is the binary LMDB database. `lock.mdb` is used by LMDB for safe
process locking. Do not edit these files manually.

If you previously ran a sled-backed build, remove the old data directory before
starting this LMDB build:

```bash
rm -rf ./data/paqus
```

## Create Wallet

```bash
cargo run -- wallet new wallet.json
```

## Run Bootstrap Node

Replace the IPv6 address with the stable public IPv6 address of the machine that
will be reachable by other nodes.

```bash
cargo run -- node run ./data/paqus \
  --listen '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:30333' \
  --rpc-listen 127.0.0.1:9933 \
  --gateway '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:8080' \
  --public-addr '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:30333' \
  --wallet wallet.json \
  --mine
```

## Run Gateway

The gateway is a separate crate. It keeps a registry of active peers so new
nodes can discover the current network.

```bash
cd ../../paqus-gateway
cargo run -- \
  --listen '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:8080' \
  --node-rpc 127.0.0.1:9933 \
  --allow-private-peers
```

Check gateway peers:

```bash
curl 'http://[2404:8000:1044:4d8:822b:f9ff:fee2:365]:8080/v1/peers?chain_id=1'
```

## Run Another Node

Use the gateway to discover peers:

```bash
cargo run -- node run ./data/paqus \
  --listen '[::]:30333' \
  --rpc-listen 127.0.0.1:9933 \
  --gateway '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:8080' \
  --public-addr '[YOUR_PUBLIC_IPV6]:30333' \
  --wallet wallet.json
```

Or connect to a bootstrap peer explicitly:

```bash
cargo run -- node run ./data/paqus \
  --listen '[::]:30333' \
  --rpc-listen 127.0.0.1:9933 \
  --gateway '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:8080' \
  --public-addr '[YOUR_PUBLIC_IPV6]:30333' \
  --peer '[2404:8000:1044:4d8:822b:f9ff:fee2:365]:30333' \
  --wallet wallet.json
```

## Useful RPC

```bash
curl http://127.0.0.1:9933/status
curl http://127.0.0.1:9933/peers
curl http://127.0.0.1:9933/accounts
curl http://127.0.0.1:9933/blocks/latest
```

More command examples are in [COMMANDS.md](./COMMANDS.md).
