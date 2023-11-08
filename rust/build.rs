use ethers::prelude::Abigen;

fn main() {
    Abigen::new("UniswapV2", "./abis/UniswapV2.json")
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file("./src/contracts/uniswap_v2.rs")
        .unwrap();
}
