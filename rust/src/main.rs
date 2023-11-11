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
use log::{debug, info};
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
        token_0, token_1, ..
    } = token_pair;

    // FIXME: Don't use get_amounts_out maybe, so that we don't calculate it with the fee?
    let request = uniswap_router.get_amounts_out(
        U256::from(1) * U256::exp10(token_0.decimals as usize),
        vec![token_1.address, token_0.address],
    );
    let results = request.call().await.unwrap();
    let price = Decimal::from(results[1].as_u64())
        / Decimal::from(10)
            .checked_powu(token_pair.token_1.decimals)
            .unwrap();

    info!("{}/{} price={}", token_0.symbol, token_1.symbol, price);
    info!(
        "{}/{} price={}",
        token_1.symbol,
        token_0.symbol,
        Decimal::ONE / price
    );

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
// Note: token_to_sell must always be larger than token_to_buy for this function
async fn get_profitable_token_swap_amounts(
    provider: Arc<Provider<Http>>,
    uniswap_factory: &UniswapFactory,
    token_to_sell: &Token,
    token_to_buy: &Token,
) -> (Decimal, Decimal, Vec<Address>) {
    // Get the pair from the factory
    let uniswap_pair_address_request =
        uniswap_factory.get_pair(token_to_sell.address, token_to_buy.address);
    let uniswap_pair_address = uniswap_pair_address_request.call().await.unwrap();
    let pair = UniswapV2Pair::new(uniswap_pair_address, provider.clone());

    // Compute price from reserve volumes
    let (raw_vol1, raw_vol2, _timestamp) = pair.get_reserves().call().await.unwrap();
    let vol1 = Decimal::from(raw_vol1)
        / Decimal::from(10)
            .checked_powu(token_to_sell.decimals)
            .unwrap();
    let vol2 = Decimal::from(raw_vol2)
        / Decimal::from(10)
            .checked_powu(token_to_buy.decimals)
            .unwrap();
    let price_from_vols = vol1 / vol2;
    debug!("Reserve balances.. t0={vol1}, t1={vol2}, price={price_from_vols}");

    let distance = vol1 - vol2;
    let half_distance = distance / Decimal::from(2);

    let quantity_to_buy = half_distance;
    (
        // FIXME: Use real math to figure out the number to use..
        quantity_to_buy * Decimal::from_str_exact("0.8").unwrap(),
        Decimal::ZERO,
        // quantity_to_buy,
        vec![token_to_sell.address, token_to_buy.address],
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
    let keeper_wallet_address: Address = keeper_wallet.address();
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
            // Price is
            let dex_price = get_dex_price(&uniswap_router, token_pair).await;

            // Adapter name
            let mut adapter_name = [0u8; 32];
            let adapter_name_string = config.adapter_name.clone();
            adapter_name[..adapter_name_string.as_bytes().len()]
                .copy_from_slice(adapter_name_string.as_bytes());

            let deadline = get_swap_deadline_from_now();

            // Determine action to take
            let maybe_tx_func = match dex_price.cmp(&Decimal::ZERO) {
                Ordering::Greater => {
                    if dex_price < config.ratio_range_allowed.0 {
                        // Price is within ignore range, noop
                        None
                    } else {
                        // Greater means ZAI (token1) is worth more than USDC (token0), so we should mint more ZAI (token1) and buy USDC (token0) via expand_and_buy
                        let (amount_in, amount_out_min, path) = get_profitable_token_swap_amounts(
                            provider.clone(),
                            &uniswap_factory,
                            &token_pair.token_1, // ZAI
                            &token_pair.token_0, // USDC
                        )
                        .await;

                        // FIXME: Determine which one to use.. with structure or raw
                        let swap_exact_tokens_for_tokens = uniswap_router
                            .swap_exact_tokens_for_tokens(
                                decimal_to_u256(amount_in, token_pair.token_1.decimals), // ZAI
                                decimal_to_u256(amount_out_min, token_pair.token_0.decimals), // USDC
                                path.clone(),
                                keeper_wallet_address,
                                deadline,
                            );
                        let delegate_call_data = swap_exact_tokens_for_tokens.calldata().unwrap();

                        debug!("SWAP_EXACT_TOKENS_FOR_TOKENS CALL, amount_in: {amount_in}, amount_out_min: {amount_out_min}, path: {path:?}, to: {keeper_wallet_address}, deadline: {deadline}");
                        debug!(
                            "SWAP_EXACT_TOKENS_FOR_TOKENS delegate_call_data={delegate_call_data}",
                        );
                        debug!("EXPAND_AND_BUY CALL, adapter_name: {adapter_name:?}, amount_in: {amount_in}, amount_out_min: {amount_out_min}, path: {path:?}, to: {keeper_wallet_address}, deadline: {deadline}");

                        Some(stability_module.expand_and_buy(
                            adapter_name,
                            delegate_call_data,
                            decimal_to_u256(amount_in, token_pair.token_1.decimals),
                        ))
                    }
                }
                Ordering::Less => {
                    if dex_price > config.ratio_range_allowed.1 {
                        // Price is within ignore range, noop
                        None
                    } else {
                        // Less means USDC (token0) is worth more than ZAI (token1), so we should be buying ZAI (token1) and burning it via contract_and_sell
                        let (amount_in, amount_out_min, path) = get_profitable_token_swap_amounts(
                            provider.clone(),
                            &uniswap_factory,
                            &token_pair.token_0, // USDC
                            &token_pair.token_1, // ZAI
                        )
                        .await;

                        let swap_exact_tokens_for_tokens = uniswap_router
                            .swap_exact_tokens_for_tokens(
                                decimal_to_u256(amount_in, token_pair.token_0.decimals), // USDC
                                decimal_to_u256(amount_out_min, token_pair.token_1.decimals), // ZAI
                                path.clone(),
                                keeper_wallet_address,
                                deadline,
                            );
                        let calldata = swap_exact_tokens_for_tokens.calldata().unwrap();
                        debug!("swap_exact_tokens_for_tokens data={calldata}",);
                        info!("CONTRACT_AND_SELL CALL, adapter_name: {adapter_name:?}, amount_in: {amount_in}, amount_out_min: {amount_out_min}, path: {path:?}, to: {keeper_wallet_address}, deadline: {deadline}");
                        Some(stability_module.contract_and_sell(adapter_name, calldata))
                    }
                }
                _ => {
                    // They are equal, no action to take
                    None
                }
            };

            match maybe_tx_func {
                None => {
                    // No favourable swap to make
                    info!("There was no favourable swap to make for dex_price of {dex_price}");
                }
                Some(tx_func) => {
                    // Wallet ethereum balance
                    let balance_int = provider
                        .get_balance(keeper_wallet.address(), None)
                        .await
                        .unwrap()
                        .as_u64();
                    let balance =
                        Decimal::from(balance_int) / Decimal::from(10).checked_powu(18).unwrap();
                    info!("Current wallet balance: {balance}");

                    // Perform the transaction
                    keeper_wallet.sign_transaction_sync(&tx_func.tx).unwrap();
                    let error = tx_func.send().await.unwrap_err();
                    println!("Error during function execution: {}", error);

                    // TODO: Save that we executed an action this block and can skip subsequent ones for this block..
                    // TODO: Wait for the outcome?
                }
            }
        }

        info!("Sleeping for {}ms", config.delay_between_checks_ms);
        thread::sleep(delay_between_checks);
    }
}
