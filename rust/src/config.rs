use super::token::{Token, TokenPair};
use ethers::abi::Address;
use rust_decimal::Decimal;
use std::env;

pub struct Config {
    pub rpc_url: String,
    pub delay_between_checks_ms: i32,
    pub token_pairs: Vec<TokenPair>,
    pub hardcoded_redemption_value: Decimal,
    pub keeper_wallet_private_key: String,
    pub uniswap_router_address: Address,
    pub uniswap_factory_address: Address,
    pub uniswap_fee_rate: Decimal,
    pub stability_module_address: Address,
}

pub fn generate_config() -> Config {
    let usdc = Token {
        symbol: String::from("USDC"),
        address: env::var("USDC_ADDRESS")
            .expect("USDC_ADDRESS environment variable not set")
            .parse()
            .expect("USDC_ADDRESS not a valid address"),
        decimals: 6,
    };

    let zai = Token {
        symbol: String::from("ZAI"),
        address: env::var("ZAI_ADDRESS")
            .expect("ZAI_ADDRESS environment variable not set")
            .parse()
            .expect("USDT_ADDRESS not a valid address"),
        decimals: 6,
    };

    let token_pairs = vec![TokenPair {
        symbol: String::from("ZAI/USDC"),
        token_in: zai,
        token_out: usdc,
    }];

    let uniswap_fee_rate_string =
        env::var("UNISWAP_FEE_RATE").expect("UNISWAP_FEE_RATE environment variable not set");

    let stability_module_address: Address = env::var("STABILITY_MODULE_ADDRESS")
        .expect("STABILITY_MODULE_ADDRESS environment variable not set")
        .parse()
        .expect("STABILITY_MODULE_ADDRESS is not valid");

    Config {
        rpc_url: env::var("RPC_URL").expect("RPC_URL environment variable not set"),
        keeper_wallet_private_key: env::var("KEEPER_WALLET_PRIVATE_KEY")
            .expect("KEEPER_WALLET_PRIVATE_KEY environment variable not set")
            .parse()
            .expect("KEEPER_WALLET_PRIVATE_KEY not a valid address"),
        uniswap_router_address: env::var("UNISWAP_ROUTER_ADDRESS")
            .expect("UNISWAP_ROUTER_ADDRESS environment variable not set")
            .parse()
            .expect("UNISWAP_ROUTER_ADDRESS not a valid address"),
        uniswap_factory_address: env::var("UNISWAP_FACTORY_ADDRESS")
            .expect("UNISWAP_FACTORY_ADDRESS environment variable not set")
            .parse()
            .expect("UNISWAP_FACTORY_ADDRESS not a valid address"),
        delay_between_checks_ms: 3_000,
        hardcoded_redemption_value: Decimal::from(1),
        uniswap_fee_rate: Decimal::from_str_exact(uniswap_fee_rate_string.as_str()).unwrap(),
        token_pairs,
        stability_module_address,
    }
}
