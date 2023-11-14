use ethers::types::Address;
use rust_decimal::Decimal;

use super::token::Token;

#[derive(Clone)]
pub struct SwapDetails {
    pub dex_price: Decimal,
    pub token_to_sell: Token,
    pub amount_to_sell: Decimal,
    pub token_to_buy: Token,
    pub amount_to_buy_min: Decimal,
    pub path: Vec<Address>,
}
