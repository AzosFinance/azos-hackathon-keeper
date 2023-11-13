mod config;
mod contracts;
mod token;

use anyhow::Result;
use config::Config;
use contracts::azos_adapter_uniswap_v2::AzosAdapterUniswapV2;
use contracts::azos_stability_module::AzosStabilityModule;
use contracts::azos_stability_module::AzosStabilityModuleErrors;
use contracts::uniswap_v2_factory::UniswapV2Factory;
use contracts::uniswap_v2_pair::UniswapV2Pair;
use contracts::uniswap_v2_router02::UniswapV2Router02;
use ethers::abi::AbiEncode;
use ethers::abi::Address;
use ethers::abi::{encode, Token as EthersToken};
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use ethers::utils::format_bytes32_string;
use log::{debug, info};
use rust_decimal::{Decimal, MathematicalOps};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{thread, time};
use token::{Token, TokenPair};

type KeeperProvider = SignerMiddleware<Provider<Http>, LocalWallet>;
type UniswapRouter = UniswapV2Router02<KeeperProvider>;
type UniswapFactory = UniswapV2Factory<KeeperProvider>;
type AzosUniswapAdapter = AzosAdapterUniswapV2<KeeperProvider>;
type StabilityModule = AzosStabilityModule<KeeperProvider>;

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
    debug!("decimal_to_u256, dec={dec}, decimals={decimals}, rounded={rounded}");
    U256::from_dec_str(rounded.to_string().as_str()).unwrap()
}

async fn get_token_swap_details(
    config: &Config,
    provider: &Arc<KeeperProvider>,
    uniswap_factory: &UniswapFactory,
    token_pair: &TokenPair,
) -> (Decimal, Decimal, Vec<Address>) {
    // Get the pair from the factory
    let uniswap_pair_address_request =
        uniswap_factory.get_pair(token_pair.token_0.address, token_pair.token_1.address);
    let uniswap_pair_address = uniswap_pair_address_request.call().await.unwrap();
    let pair = UniswapV2Pair::new(uniswap_pair_address, provider.clone());

    // Compute price from reserve supplies
    // FIXME: Make these supply0 and 1 in order
    let (raw_supply_0, raw_supply_1, _timestamp) = pair.get_reserves().call().await.unwrap();
    let supply_0 = Decimal::from(raw_supply_0)
        / Decimal::from(10)
            .checked_powu(token_pair.token_0.decimals)
            .unwrap();
    let supply_1 = Decimal::from(raw_supply_1)
        / Decimal::from(10)
            .checked_powu(token_pair.token_1.decimals)
            .unwrap();

    // Compute the price based on reserve supplies
    let total_supply = supply_0 + supply_1;
    let price_from_supplies = supply_0 / supply_1;
    let system_coin_is_worth_more = price_from_supplies > Decimal::ONE;
    debug!("Reserve balances.. t0={supply_0}, t1={supply_1}, price={price_from_supplies}");

    // Determine amount to buy/sell based on a goal ratio
    let goal_ratio = if system_coin_is_worth_more {
        config.ratio_range_targets.1
    } else {
        config.ratio_range_targets.0
    };

    let expected_buy_token_supply = (total_supply / Decimal::TWO)
        + (((goal_ratio - Decimal::ONE) / Decimal::TWO.powu(2)) * total_supply);

    let (quantity_to_sell, quantity_to_buy, path) = if system_coin_is_worth_more {
        let quantity_to_buy = supply_0 - expected_buy_token_supply;
        let quantity_to_sell = quantity_to_buy;
        let path = vec![token_pair.token_1.address, token_pair.token_0.address];
        (quantity_to_sell, quantity_to_buy, path)
    } else {
        let quantity_to_buy = expected_buy_token_supply - supply_0;
        let quantity_to_sell = quantity_to_buy;
        let path = vec![token_pair.token_0.address, token_pair.token_1.address];
        (quantity_to_sell, quantity_to_buy, path)
    };
    let resulting_ratio = (supply_0 + quantity_to_sell) / (supply_1 + quantity_to_buy);
    debug!("PROFITABLE TOKEN SWAP AMOUNTS, expected_resulting_supply={expected_buy_token_supply}, quantity_to_sell={quantity_to_sell}, quantity_to_buy={quantity_to_buy}, path={path:?}");
    debug!("RESULTING RATIO, {}", resulting_ratio);
    (quantity_to_sell, quantity_to_buy, path)
}

#[derive(Clone)]
struct SwapDetails {
    dex_price: Decimal,
    token_to_sell: Token,
    amount_to_sell: Decimal,
    token_to_buy: Token,
    amount_to_buy_min: Decimal,
    path: Vec<Address>,
}

#[derive(Clone)]
enum KeeperAction {
    ExpandAndBuy(SwapDetails),
    ContractAndSell(SwapDetails),
    None(SwapDetails),
}

async fn determine_action_to_take_for_pair(
    config: &Config,
    provider: &Arc<KeeperProvider>,
    uniswap_router: &UniswapRouter,
    uniswap_factory: &UniswapFactory,
    token_pair: &TokenPair,
) -> KeeperAction {
    let dex_price = get_dex_price(uniswap_router, token_pair).await;

    if dex_price >= config.ratio_range_allowed.0 && dex_price <= config.ratio_range_allowed.1 {
        KeeperAction::None(SwapDetails {
            dex_price,
            token_to_sell: token_pair.token_0.clone(),
            amount_to_sell: Decimal::ZERO,
            token_to_buy: token_pair.token_1.clone(),
            amount_to_buy_min: Decimal::ZERO,
            path: vec![],
        })
    } else if dex_price > Decimal::ZERO {
        // System coin is worth more than stable coin
        let (amount_to_sell, amount_to_buy_min, path) =
            get_token_swap_details(config, provider, uniswap_factory, token_pair).await;
        KeeperAction::ExpandAndBuy(SwapDetails {
            dex_price,
            token_to_sell: token_pair.token_1.clone(),
            amount_to_sell,
            token_to_buy: token_pair.token_1.clone(),
            amount_to_buy_min,
            path,
        })
    } else {
        // Stable coin is worth more than system coin
        let (amount_to_sell, amount_to_buy_min, path) =
            get_token_swap_details(config, provider, uniswap_factory, token_pair).await;
        KeeperAction::ContractAndSell(SwapDetails {
            dex_price,
            token_to_sell: token_pair.token_0.clone(),
            amount_to_sell,
            token_to_buy: token_pair.token_0.clone(),
            amount_to_buy_min,
            path,
        })
    }
}

fn generate_delegate_call_data(
    config: &Config,
    uniswap_adapter: &AzosAdapterUniswapV2<KeeperProvider>,
    swap_details: &SwapDetails,
) -> Bytes {
    let deadline = get_swap_deadline_from_now();
    let adapter_swap_args: [EthersToken; 5] = vec![
        EthersToken::Uint(decimal_to_u256(
            swap_details.amount_to_sell,
            swap_details.token_to_sell.decimals,
        )), // ZAI
        EthersToken::Uint(decimal_to_u256(
            swap_details.amount_to_buy_min,
            swap_details.token_to_buy.decimals,
        )), // USDC
        EthersToken::Array(
            swap_details
                .path
                .iter()
                .map(|a| EthersToken::Address(*a))
                .collect(),
        ), // Path
        EthersToken::Uint(deadline),                         // Deadline
        EthersToken::Address(config.uniswap_router_address), // Router
    ]
    .try_into()
    .unwrap();
    debug!("adapter_swap_args={adapter_swap_args:?}");
    let adapter_swap_data: Bytes = encode(&adapter_swap_args).into();
    let adapter_swap_call = uniswap_adapter.swap(adapter_swap_data);
    adapter_swap_call.calldata().unwrap()
}

async fn tick_keeper_loop(
    config: &Config,
    provider: &Arc<KeeperProvider>,
    uniswap_router: &UniswapRouter,
    uniswap_factory: &UniswapFactory,
    uniswap_adapter: &AzosUniswapAdapter,
    stability_module: &StabilityModule,
) {
    let adapter_name = format_bytes32_string(config.adapter_name.as_str()).unwrap();
    for token_pair in &config.token_pairs {
        let action_to_take = determine_action_to_take_for_pair(
            config,
            provider,
            uniswap_router,
            uniswap_factory,
            token_pair,
        )
        .await;
        match &action_to_take {
            KeeperAction::ContractAndSell(swap_details)
            | KeeperAction::ExpandAndBuy(swap_details) => {
                // Do the right contract/expand call
                let delegate_call_data =
                    generate_delegate_call_data(config, uniswap_adapter, swap_details);

                let adapter_name_as_hex = adapter_name.encode_hex();
                let stability_module_call = if let KeeperAction::ContractAndSell(_) =
                    &action_to_take
                {
                    debug!("CONTRACT_AND_SELL, adapter_name={adapter_name_as_hex:?}, data={delegate_call_data}");
                    stability_module.contract_and_sell(adapter_name, delegate_call_data)
                } else {
                    let mint_amount = decimal_to_u256(
                        swap_details.amount_to_sell,
                        swap_details.token_to_sell.decimals,
                    );
                    debug!("EXPAND_AND_BUY CALL, adapter_name={adapter_name_as_hex:?}, data={delegate_call_data}, mint_amount={mint_amount}");
                    stability_module.expand_and_buy(adapter_name, delegate_call_data, mint_amount)
                };

                // Wallet ethereum balance
                let balance_int = provider
                    .get_balance(provider.address(), None)
                    .await
                    .unwrap()
                    .as_u64();
                let balance =
                    Decimal::from(balance_int) / Decimal::from(10).checked_powu(18).unwrap();
                info!("Current wallet balance: {balance}");

                // Broadcast the transaction
                let call_result = stability_module_call.send().await;
                match call_result {
                    Ok(_) => {
                        info!("Successful transaction!");
                    }
                    Err(error) => {
                        println!("Error during function execution: {}", error);
                        let contract_revert: AzosStabilityModuleErrors =
                            error.decode_contract_revert().unwrap();
                        println!("Contract revert: {:?}", contract_revert);
                    }
                }

                // TODO: Save that we executed an action this block and can skip subsequent ones for this block..
                // TODO: Wait for the outcome?
            }
            KeeperAction::None(swap_details) => {
                info!(
                    "There was no favourable swap to make for dex_price of {}",
                    swap_details.dex_price
                );
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    dotenv::dotenv().ok();

    info!("Starting up..");
    let config = config::generate_config();

    // Provider, Wallet, and Signer Client
    let provider = Provider::<Http>::try_from(config.rpc_url.clone()).unwrap();
    let keeper_wallet: LocalWallet = config
        .keeper_wallet_private_key
        .parse::<LocalWallet>()?
        // FIXME: Make this chain configured from env var
        .with_chain_id(Chain::Sepolia);
    let provider = SignerMiddleware::new(provider.clone(), keeper_wallet.clone());
    let provider = Arc::new(provider);

    // Uniswap
    let uniswap_router = UniswapV2Router02::new(config.uniswap_router_address, provider.clone());
    let uniswap_factory = UniswapV2Factory::new(config.uniswap_factory_address, provider.clone());

    // Stability Module
    let stability_module =
        AzosStabilityModule::new(config.stability_module_address, provider.clone());
    let uniswap_adapter =
        AzosAdapterUniswapV2::new(config.adapter_uniswap_v2_address, provider.clone());

    // Core loop
    info!("Configuration loaded, initiating keeper loop");
    let delay_between_checks = time::Duration::from_millis(config.delay_between_checks_ms as u64);
    loop {
        tick_keeper_loop(
            &config,
            &provider,
            &uniswap_router,
            &uniswap_factory,
            &uniswap_adapter,
            &stability_module,
        )
        .await;

        info!("Sleeping for {}ms", config.delay_between_checks_ms);
        thread::sleep(delay_between_checks);
    }
}
