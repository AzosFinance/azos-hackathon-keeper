use super::swap::SwapDetails;

#[derive(Clone)]
pub enum KeeperAction {
    ExpandAndBuy(SwapDetails),
    ContractAndSell(SwapDetails),
    None(SwapDetails),
}
