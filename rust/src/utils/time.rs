use std::time::{Duration, SystemTime};

use ethers::types::U256;

pub fn get_swap_deadline_from_now() -> U256 {
    let future = SystemTime::now() + Duration::from_secs(120);
    let future_timestamp = future
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    U256::from(future_timestamp)
}