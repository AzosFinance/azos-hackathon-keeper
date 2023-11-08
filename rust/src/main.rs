mod config;
mod contracts;
mod token;

use config::Config;
use contracts::uniswap_v2::UniswapV2;
use ethers::abi::Address;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use log::info;
use rust_decimal::{Decimal, MathematicalOps};
use std::cmp::Ordering;
use std::sync::Arc;
use std::{thread, time};
use token::TokenPair;

async fn get_dex_price(
    config: &Config,
    uniswap: &UniswapV2<Provider<Http>>,
    pair: &TokenPair,
) -> Decimal {
    let in_address: Address = pair.token_in.address.parse().unwrap();
    let out_address: Address = pair.token_out.address.parse().unwrap();
    let request = uniswap.get_amounts_out(
        U256::from(1) * U256::exp10(pair.token_in.decimals as usize),
        vec![in_address, config.weth_address, out_address],
    );
    let results = request.call().await.unwrap();

    Decimal::from(results[2].as_u64())
        / Decimal::from(10)
            .checked_powu(pair.token_in.decimals)
            .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    dotenv::dotenv().ok();

    info!("Starting up..");
    let config = config::generate_config();

    let provider = Provider::<Http>::try_from(config.rpc_url.clone()).unwrap();

    let uniswap_address = config
        .uniswap_router_address
        .parse::<Address>()
        .expect("Provided Uniswap address is not valid");
    let uniswap = UniswapV2::new(uniswap_address, Arc::new(provider));

    info!("Configuration loaded, initiating keeper loop");
    let delay_between_checks = time::Duration::from_millis(config.delay_between_checks_ms as u64);
    loop {
        for pair in &config.token_pairs {
            let internal_price = config.hardcoded_redemption_value;
            let dex_price = get_dex_price(&config, &uniswap, pair).await;
            let price_gap = internal_price - dex_price;
            // FIXME: Make the gap incorporate the adapter_fee_rate

            match price_gap.cmp(&Decimal::ZERO) {
                Ordering::Greater => {
                    info!("Price gap > 0, expandAndBuy");
                }
                Ordering::Less => {
                    info!("Price gap < 0, contractAndSell");
                }
                Ordering::Equal => {
                    // Values are equal, noop
                }
            }
        }

        thread::sleep(delay_between_checks);
    }
}
