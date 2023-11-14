use ethers::types::U256;
use log::debug;
use rust_decimal::{Decimal, MathematicalOps};

pub fn decimal_to_u256(dec: Decimal, decimals: u64) -> U256 {
    let rounded = (dec * Decimal::from(10).checked_powu(decimals).unwrap()).floor();
    debug!("decimal_to_u256, dec={dec}, decimals={decimals}, rounded={rounded}");
    U256::from_dec_str(rounded.to_string().as_str()).unwrap()
}


/**
 * If the dex_price is within the allowed range, we should "ignore" it in the sense of not taking action.
 */
pub fn decimal_is_within_allowed_range(price: Decimal, allowed_range: (Decimal, Decimal)) -> bool {
    price >= allowed_range.0 && price <= allowed_range.1
}