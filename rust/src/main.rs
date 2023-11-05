use ethers::abi::Address;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use log::info;
use rust_decimal::{Decimal, MathematicalOps};
use std::sync::Arc;
use std::{env, thread, time};

struct Config {
    rpc_url: String,
    uniswap_router_address: String,
    delay_between_checks_ms: i32,
    token_pairs: Vec<TokenPair>,
}

#[derive(Clone)]
struct Token {
    symbol: String,
    address: String,
    decimals: u64,
}

#[derive(Clone)]
struct TokenPair {
    token_in: Token,
    token_out: Token,
}

// TODO: Etherscan rate-limits requests to their API. To avoid this, set the ETHERSCAN_API_KEY environment variable.
abigen!(UniswapV2, "./src/abis/uniswapV2.json");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    dotenv::dotenv().ok();

    info!("Starting up..");
    // FIXME: Move to a config.rs file or similar?
    let weth_address: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap();
    let usdc = Token {
        symbol: String::from("USDC"),
        // FIXME: Convert to Address instances?
        address: String::from("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        decimals: 6,
    };
    let usdt = Token {
        symbol: String::from("USDT"),
        address: String::from("0xdac17f958d2ee523a2206206994597c13d831ec7"),
        decimals: 6,
    };
    let coin_pairs = vec![
        // FIXME: Should we clone or make an Rc or something?
        TokenPair {
            token_in: usdc.clone(),
            token_out: usdt.clone(),
        },
        TokenPair {
            token_in: usdt.clone(),
            token_out: usdc.clone(),
        },
    ];
    let config = Config {
        rpc_url: env::var("RPC_URL").expect("RPC_URL environment variable not set"),
        // FIXME: Replace with env var
        uniswap_router_address: String::from("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"), // WETH
        delay_between_checks_ms: 3_000,
        token_pairs: coin_pairs,
    };

    let provider = Provider::<Http>::try_from(config.rpc_url).unwrap();
    let address = config
        .uniswap_router_address
        .parse::<Address>()
        .expect("Provided Uniswap address is not valid");
    let uniswap = UniswapV2::new(address, Arc::new(provider));

    info!("Configuration loaded, initiating keeper loop");
    let delay_between_checks = time::Duration::from_millis(config.delay_between_checks_ms as u64);
    loop {
        for pair in &config.token_pairs {
            let in_address: Address = pair.token_in.address.parse().unwrap();
            let out_address: Address = pair.token_out.address.parse().unwrap();
            let request = uniswap.get_amounts_out(
                U256::from(1) * U256::exp10(pair.token_in.decimals as usize),
                vec![in_address, weth_address, out_address],
            );
            let results = request.call().await?;
            let value = Decimal::from(results[2].as_u64())
                / Decimal::from(10)
                    .checked_powu(pair.token_in.decimals)
                    .unwrap();
            info!(
                "{} => {} : {}",
                pair.token_in.symbol, pair.token_out.symbol, value
            );
        }

        thread::sleep(delay_between_checks);
    }
}
