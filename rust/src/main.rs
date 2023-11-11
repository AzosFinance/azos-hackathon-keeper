mod config;
mod contracts;
mod token;

use contracts::azos_stability_module::{AzosStabilityModule, AzosStabilityModuleErrors};
use contracts::uniswap_v2_factory::UniswapV2Factory;
use contracts::uniswap_v2_pair::UniswapV2Pair;
use contracts::uniswap_v2_router02::UniswapV2Router02;
use ethers::abi::Address;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use log::{error, info};
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
    info!("Inverse price={}", Decimal::ONE / price);

    price
}

fn get_swap_deadline_from_now() -> U256 {
    let future = SystemTime::now() + Duration::from_secs(120);
    let future_timestamp = future
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    U256::from(future_timestamp)
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

    let token0 = pair.token_0().call().await.unwrap();
    let token1 = pair.token_1().call().await.unwrap();
    info!("t0:{}, t1:{}", token0, token1);

    // TODO: Figure out how much volume there is of both tokens in the pool
    let (raw_vol1, raw_vol2, _timestamp) = pair.get_reserves().call().await.unwrap();
    let vol1 = Decimal::from(raw_vol1) / Decimal::from(10).checked_powu(token_in.decimals).unwrap();
    let vol2 =
        Decimal::from(raw_vol2) / Decimal::from(10).checked_powu(token_out.decimals).unwrap();
    let price_from_vols = vol2 / vol1;
    info!(
        "{}/{} RESERVES: {}, {}",
        token_in.symbol, token_out.symbol, vol1, vol2
    );

    // TODO: Determine how much to attempt to buy
    // FIXME: Remove this 90% modifier
    let quantity_to_buy = vol1 - vol2;
    let quantity_to_buy_using_ratios = (Decimal::ONE - price_from_vols) * vol2;
    info!(
        "How many to buy? Known qual qty={}, using ratios={}",
        quantity_to_buy, quantity_to_buy_using_ratios
    );
    // TODO: Incorporate slippage somehow
    // TODO: Incorporate gas price somehow.. maybe outside this function
    let quantity_to_buy = Decimal::from_str_exact("50000").unwrap();

    (
        quantity_to_buy,
        Decimal::ZERO,
        // quantity_to_buy,
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
            let dex_price = get_dex_price(&uniswap_router, token_pair).await;

            // Adapter name
            // FIXME: Use env var for selecting adapter, or configure in config.rs
            let mut adapter_name = [0u8; 32];
            // FIXME: Change when typo fixed in contract
            // let adapter_name_string = String::from("UniswapV2");
            let adapter_name_string = String::from("USDC");
            adapter_name[..adapter_name_string.as_bytes().len()]
                .copy_from_slice(adapter_name_string.as_bytes());

            // FIXME: Be less verbose in each branch, compact the code a bit
            let tx_func = match dex_price.cmp(&Decimal::ZERO) {
                Ordering::Less => {
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
                    println!("{}", swap_exact_tokens_for_tokens.calldata().unwrap());
                    stability_module.expand_and_buy(
                        adapter_name,
                        swap_exact_tokens_for_tokens.calldata().unwrap(),
                        decimal_to_u256(amount_in, token_pair.token_in.decimals),
                    )
                }
                _ => {
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
                    stability_module.contract_and_sell(
                        adapter_name,
                        swap_exact_tokens_for_tokens.calldata().unwrap(),
                    )
                }
            };

            // Wallet ethereum balance
            let balance_int = provider
                .get_balance(keeper_wallet.address(), None)
                .await
                .unwrap()
                .as_u64();
            let balance = Decimal::from(balance_int) / Decimal::from(10).checked_powu(18).unwrap();
            info!("Current wallet balance: {balance}");

            // Perform the transaction
            keeper_wallet.sign_transaction_sync(&tx_func.tx).unwrap();
            let error = tx_func.send().await.unwrap_err();
            println!("{}", error);
            let revert_reason: contracts::azos_stability_module::AzosStabilityModuleErrors =
                error.decode_contract_revert().unwrap();

            // TODO: Save that we executed an action this block and can skip subsequent ones for this block..
            // TODO: Wait for the outcome?
        }

        thread::sleep(delay_between_checks);
    }
}
