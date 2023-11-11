use ethers::abi::Address;

#[derive(Clone)]
pub struct Token {
    pub symbol: String,
    pub address: Address,
    pub decimals: u64,
}

#[derive(Clone)]
pub struct TokenPair {
    pub symbol: String,
    pub token_0: Token,
    pub token_1: Token,
}
