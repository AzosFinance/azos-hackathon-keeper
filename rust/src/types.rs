use ethers::abi::Address;

pub struct Config {
    pub rpc_url: String,
    pub uniswap_router_address: String,
    pub delay_between_checks_ms: i32,
    pub token_pairs: Vec<TokenPair>,
    pub weth_address: Address,
}

#[derive(Clone)]
pub struct Token {
    pub symbol: String,
    pub address: String,
    pub decimals: u64,
}

#[derive(Clone)]
pub struct TokenPair {
    pub token_in: Token,
    pub token_out: Token,
}
