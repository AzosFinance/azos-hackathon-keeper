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
