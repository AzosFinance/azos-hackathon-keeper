use ethers::abi::Address;

#[derive(Clone)]
pub struct Token {
    pub symbol: String,
    pub address: Address,
    pub decimals: u64,
}

#[derive(Clone)]
pub struct TokenPair {
    pub token_in: Token,
    pub token_out: Token,
}
