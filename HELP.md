curl -L \
  https://github.com/pyth-network/pyth-client/releases/download/oracle-v2.35.0/pyth_oracle_pythnet.so \
  -o ./pyth_client.so

solana-test-validator --reset \
  --bpf-program FsSMpvcnNL7ewh2nCS5M1XsU1QUoAVDq4VpG7s1N4TxE ./pyth_client.so

cargo run --bin client new-mint --decimals 8
cargo run --bin client new-mint --decimals 8

cargo run --bin client new-token DtEbmakmWcLLwsc9ZCeqZz6enqGHUxSKHvar5A676nGu 5JwjUhSgwnp1VQ12tjEp3xFPsbSvvu4WknGZt42gfAa1
cargo run --bin client new-token 7CL7ZjAbPmGqvcGVSkSnkwZA1H5x6MHKDKgGtuJffdbb 5JwjUhSgwnp1VQ12tjEp3xFPsbSvvu4WknGZt42gfAa1

cargo run --bin client mint-to 7CL7ZjAbPmGqvcGVSkSnkwZA1H5x6MHKDKgGtuJffdbb DXnbJ1dRc32hXJxEyTisCeuNwLxGhcKdyUzhquXHsUjW 100000000000000
cargo run --bin client mint-to DtEbmakmWcLLwsc9ZCeqZz6enqGHUxSKHvar5A676nGu 8vDtdceEwmiHreeGWwwu7JxHeBU8HAwGKJoqTu2iMBNK 200000000000000

cargo run --bin client create-config 1 0 0 0 0 0 9999999
cargo run --bin client create-pool 1 1.1 7CL7ZjAbPmGqvcGVSkSnkwZA1H5x6MHKDKgGtuJffdbb DtEbmakmWcLLwsc9ZCeqZz6enqGHUxSKHvar5A676nGu D3964sjHDzz6vTsxSDmqbzP1Whs8sr6MdHjNTZDEZaEo
cargo run --bin client open-position FDXY8f1jxcXAiATwxZPHGe8DAZwd6n2ewvCxM9BDncbJ 0 1000000000
cargo run --bin client create-price-feed 120000 100 6