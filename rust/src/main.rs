mod config;
mod contracts;
mod token;

use config::Config;
use contracts::azos_stability_module::AzosStabilityModule;
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

    let stability_module = AzosStabilityModule::new(uniswap_address, Arc::new(provider));

    info!("Configuration loaded, initiating keeper loop");
    let delay_between_checks = time::Duration::from_millis(config.delay_between_checks_ms as u64);
    loop {
        for pair in &config.token_pairs {
            let internal_price = config.hardcoded_redemption_value;
            let dex_price = get_dex_price(&config, &uniswap, pair).await;
            // FIXME: Make the price_gap incorporate the adapter_fee_rate
            let price_gap = internal_price - dex_price;

            let adapter_name = "UniswapV2".as_bytes().try_into().unwrap();

            // FIXME: Replace these weird values with real ones
            match price_gap.cmp(&Decimal::ZERO) {
                Ordering::Greater => {
                    // FIXME: Proper values
                    let swap_exact_tokens_for_tokens = uniswap.swap_exact_tokens_for_tokens(
                        U256::from(1), //
                        U256::from(1),
                        vec![],
                        uniswap_address,
                        U256::from(1),
                    );

                    let expand_and_buy = stability_module.expand_and_buy(
                        adapter_name,
                        swap_exact_tokens_for_tokens.calldata().unwrap(),
                        U256::from(1),
                    );
                    info!("Price gap > 0, expandAndBuy call: {:?}", expand_and_buy);
                    // FIXME: Perform the call, using a wallet
                }
                Ordering::Less => {
                    // FIXME: Proper values
                    let swap_exact_tokens_for_tokens = uniswap.swap_exact_tokens_for_tokens(
                        U256::from(1), //
                        U256::from(1),
                        vec![],
                        uniswap_address,
                        U256::from(1),
                    );

                    let contract_and_sell = stability_module.contract_and_sell(
                        adapter_name,
                        swap_exact_tokens_for_tokens.calldata().unwrap(),
                    );
                    info!("Price gap < 0, contractAndSell: {:?}", contract_and_sell);
                    // FIXME: Perform the call, using a wallet
                }
                Ordering::Equal => {
                    // Values are equal, noop
                } // FIXME: Alert if a wallet balance is too low for gas fees
            }
        }

        thread::sleep(delay_between_checks);
    }
}
