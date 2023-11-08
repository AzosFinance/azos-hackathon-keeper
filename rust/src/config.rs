use super::types::{Config, Token, TokenPair};
use ethers::abi::Address;
use std::env;

pub fn generate_config() -> Config {
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
    let token_pairs = vec![
        // FIXME: Should we clone or make an Rc or something?
        TokenPair {
            token_in: usdc.clone(),
            token_out: usdt.clone(),
        },
        TokenPair {
            token_in: usdt,
            token_out: usdc,
        },
    ];

    Config {
        rpc_url: env::var("RPC_URL").expect("RPC_URL environment variable not set"),
        // FIXME: Replace with env var
        uniswap_router_address: String::from("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"), // WETH
        delay_between_checks_ms: 3_000,
        token_pairs,
        weth_address,
    }
}
