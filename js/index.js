import dotenv from "dotenv";
import { Contract, ethers, providers } from "ethers";
import { ROUTER_ABI } from "./utils/routerAbi.js";
import { WETH_ADDRESS, UNISWAP_ROUTER } from "./utils/addresses.js";
import { USDT, USDC } from "./utils/pairs.js";
dotenv.config();

const pairs = [
  { tokenIn: USDT, tokenOut: USDC },
  { tokenIn: USDC, tokenOut: USDT },
];
const provider = new providers.StaticJsonRpcProvider(
  "https://mainnet.infura.io/v3/" + process.env.ETHEREUM_RPC_INFURA_KEY
);

const routerContract = new Contract(UNISWAP_ROUTER, ROUTER_ABI, provider);

const startKeeper = () => {
  setTimeout(async () => {
    for (let i = 0; i < pairs.length; i++) {
      const tokenIn = pairs[i].tokenIn;
      const tokenOut = pairs[i].tokenOut;

      const quantityOut = await routerContract.getAmountsOut(
        ethers.utils.parseUnits("1", tokenIn.decimals),
        [tokenIn.address, WETH_ADDRESS, tokenOut.address]
      );

      console.log(
        tokenIn.symbol,
        "=>",
        tokenOut.symbol,
        ":",
        ethers.utils.formatUnits(quantityOut[2], tokenIn.decimals)
      );
    }

    console.log("-----------------------");

    startKeeper();
  }, 3000);
};

startKeeper();
