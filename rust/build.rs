use ethers::prelude::Abigen;

fn main() {
    Abigen::new("AzosStabilityModule", "./abis/AzosStabilityModule.json")
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file("./src/contracts/azos_stability_module.rs")
        .unwrap();

    Abigen::new("UniswapV2", "./abis/UniswapV2.json")
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file("./src/contracts/uniswap_v2.rs")
        .unwrap();

    Abigen::new("UniswapV2Pair", "./abis/UniswapV2Pair.json")
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file("./src/contracts/uniswap_v2_pair.rs")
        .unwrap();
}
