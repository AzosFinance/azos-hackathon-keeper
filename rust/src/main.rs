mod config;
mod contracts;
mod token;

use contracts::azos_stability_module::AzosStabilityModule;
use contracts::uniswap_v2_factory::UniswapV2Factory;
use contracts::uniswap_v2_pair::UniswapV2Pair;
use contracts::uniswap_v2_router02::UniswapV2Router02;
use ethers::abi::Address;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use log::info;
use rust_decimal::{Decimal, MathematicalOps};
use std::cmp::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{thread, time};
use token::{Token, TokenPair};

type UniswapRouter = UniswapV2Router02<Provider<Http>>;
type UniswapFactory = UniswapV2Factory<Provider<Http>>;

async fn get_dex_price(uniswap_router: &UniswapRouter, token_pair: &TokenPair) -> Decimal {
    let TokenPair {
        token_in,
        token_out,
        ..
    } = token_pair;

    let request = uniswap_router.get_amounts_out(
        U256::from(1) * U256::exp10(token_in.decimals as usize),
        vec![token_in.address, token_out.address],
    );
    let results = request.call().await.unwrap();
    let price = Decimal::from(results[1].as_u64())
        / Decimal::from(10)
            .checked_powu(token_pair.token_out.decimals)
            .unwrap();

    info!("{}/{} price={}", token_in.symbol, token_out.symbol, price);

    price
}

fn get_swap_deadline_from_now() -> U256 {
    let future = SystemTime::now() + Duration::from_secs(60 * 60 * 24);
    U256::from(
        future
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
}

fn decimal_to_u256(dec: Decimal, decimals: u64) -> U256 {
    let rounded = (dec * Decimal::from(10).checked_powu(decimals).unwrap()).floor();
    U256::from_dec_str(rounded.to_string().as_str()).unwrap()
}

// FIXME: Ensure we're always using the right order of token0 and token1 for sorting
async fn get_profitable_token_swap_amounts(
    provider: Arc<Provider<Http>>,
    uniswap_factory: &UniswapFactory,
    token_in: &Token,
    token_out: &Token,
) -> (Decimal, Decimal, Vec<Address>) {
    // Get the pair from the factory
    let uniswap_pair_address_request =
        uniswap_factory.get_pair(token_in.address, token_out.address);
    let uniswap_pair_address = uniswap_pair_address_request.call().await.unwrap();
    let pair = UniswapV2Pair::new(uniswap_pair_address, provider.clone());

    // TODO: Figure out how much volume there is of both tokens in the pool
    let (raw_vol1, raw_vol2, _timestamp) = pair.get_reserves().call().await.unwrap();
    let lp_fee_rate_remainder = Decimal::ONE - Decimal::from_str_exact("0.003").unwrap();
    let vol1 = (Decimal::from(raw_vol1) * lp_fee_rate_remainder)
        / Decimal::from(10).checked_powu(token_in.decimals).unwrap();
    let vol2 =
        Decimal::from(raw_vol2) / Decimal::from(10).checked_powu(token_out.decimals).unwrap();
    let price_from_vols = vol1 / vol2;
    info!(
        "{}/{} RESERVES: {}, {}.. price after lp fee? {}",
        token_in.symbol, token_out.symbol, vol1, vol2, price_from_vols
    );

    // TODO: Determine how much to attempt to buy
    let quantity_to_buy = vol2 - vol1;
    let quantity_to_buy_using_ratios = (Decimal::ONE - price_from_vols) * vol2;
    info!(
        "How many to buy? Known qual qty={}, using ratios={}",
        quantity_to_buy, quantity_to_buy_using_ratios
    );
    // TODO: Incorporate slippage somehow
    // TODO: Incorporate gas price somehow.. maybe outside this function

    (
        quantity_to_buy,
        quantity_to_buy,
        vec![token_in.address, token_out.address],
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    dotenv::dotenv().ok();

    info!("Starting up..");
    let config = config::generate_config();

    // Provider, Wallet, and Signer Client
    let provider = Arc::new(Provider::<Http>::try_from(config.rpc_url.clone()).unwrap());
    let keeper_wallet: LocalWallet = config
        .keeper_wallet_private_key
        .parse::<LocalWallet>()?
        // FIXME: Make this chain configured from env var
        .with_chain_id(Chain::Sepolia);
    let _client = SignerMiddleware::new(provider.clone(), keeper_wallet.clone());

    // Uniswap
    let uniswap_router = UniswapV2Router02::new(config.uniswap_router_address, provider.clone());
    let uniswap_factory = UniswapV2Factory::new(config.uniswap_factory_address, provider.clone());

    // Stability Module
    let stability_module =
        AzosStabilityModule::new(config.stability_module_address, provider.clone());

    // Core loop
    info!("Configuration loaded, initiating keeper loop");
    let delay_between_checks = time::Duration::from_millis(config.delay_between_checks_ms as u64);
    loop {
        for token_pair in &config.token_pairs {
            let internal_price = config.hardcoded_redemption_value;
            let dex_price = get_dex_price(&uniswap_router, token_pair).await;
            // FIXME: Make the price_gap incorporate the adapter_fee_rate and split to its own function
            let price_gap = internal_price - dex_price;

            // Adapter name
            // FIXME: Use env var for selecting adapter, or configure in config.rs
            let mut adapter_name = [0u8; 32];
            let adapter_name_string = String::from("UniswapV2");
            adapter_name[..adapter_name_string.as_bytes().len()]
                .copy_from_slice(adapter_name_string.as_bytes());

            // FIXME: Be less verbose in each branch, compact the code a bit
            match price_gap.cmp(&Decimal::ZERO) {
                Ordering::Greater => {
                    let (amount_in, amount_out_min, path) = get_profitable_token_swap_amounts(
                        provider.clone(),
                        &uniswap_factory,
                        &token_pair.token_in,
                        &token_pair.token_out,
                    )
                    .await;
                    let swap_exact_tokens_for_tokens = uniswap_router.swap_exact_tokens_for_tokens(
                        decimal_to_u256(amount_in, token_pair.token_in.decimals),
                        decimal_to_u256(amount_out_min, token_pair.token_out.decimals),
                        path,
                        config.stability_module_address,
                        get_swap_deadline_from_now(),
                    );
                    let expand_and_buy = stability_module.expand_and_buy(
                        adapter_name,
                        swap_exact_tokens_for_tokens.calldata().unwrap(),
                        U256::from(1),
                    );
                    info!(
                        "Price gap > 0, expandAndBuy call: {:?}",
                        expand_and_buy.calldata()
                    );
                    // FIXME: Perform the call, using a wallet
                }
                Ordering::Less => {
                    let (amount_in, amount_out_min, path) = get_profitable_token_swap_amounts(
                        provider.clone(),
                        &uniswap_factory,
                        &token_pair.token_out,
                        &token_pair.token_in,
                    )
                    .await;
                    let swap_exact_tokens_for_tokens = uniswap_router.swap_exact_tokens_for_tokens(
                        decimal_to_u256(amount_in, token_pair.token_in.decimals),
                        decimal_to_u256(amount_out_min, token_pair.token_out.decimals),
                        path,
                        config.stability_module_address,
                        get_swap_deadline_from_now(),
                    );
                    let contract_and_sell = stability_module.contract_and_sell(
                        adapter_name,
                        swap_exact_tokens_for_tokens.calldata().unwrap(),
                    );
                    info!(
                        "Price gap < 0, contractAndSell: {:?}",
                        contract_and_sell.calldata()
                    );
                    // FIXME: Perform the call, using a wallet
                }
                Ordering::Equal => {
                    // Values are equal, noop
                }
            }
        }

        thread::sleep(delay_between_checks);
    }
}
