mod config;
mod contracts;
mod types;

use contracts::uniswap_v2::UniswapV2;
use ethers::abi::Address;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use log::info;
use rust_decimal::{Decimal, MathematicalOps};
use std::sync::Arc;
use std::{thread, time};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    dotenv::dotenv().ok();

    info!("Starting up..");
    let config = config::generate_config();

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
                vec![in_address, config.weth_address, out_address],
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
