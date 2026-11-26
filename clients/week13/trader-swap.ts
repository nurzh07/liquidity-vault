import { PublicKey } from "@solana/web3.js";

import {
  swapInternal,
  getPdaExBitmapAccount,
  TickArrayBitmapExtensionLayout,
  TxVersion,
} from "@raydium-io/raydium-sdk-v2";

import BN from "bn.js";
import * as fs from "fs";

import {
  CLMM_PROGRAM,
  connection,
  initRaydium,
  owner,
  explorerTx,
} from "./trader-common";

const AMOUNT = new BN(
  process.env.AMOUNT_IN ?? "100000000"
);

const A_TO_B =
  (process.env.DIRECTION ?? "a2b") === "a2b";

async function main() {
  const info = JSON.parse(
    fs.readFileSync("pool.json", "utf8")
  );

  const poolId = new PublicKey(info.poolId);

  const raydium = await initRaydium();

  const originalValues = Object.values;

  let pool: any;

  try {
    (Object as any).values = (o: any) =>
      o == null ? [] : originalValues(o);

    pool = await raydium.clmm.getPoolInfoFromRpc(
      poolId.toBase58()
    );
  } finally {
    (Object as any).values = originalValues;
  }

  const { poolInfo, poolKeys } = pool;

  const inputMint = A_TO_B
    ? poolInfo.mintA.address
    : poolInfo.mintB.address;

  const {
    rpcData,
    tickArrays,
    configInfo,
  } = await raydium.clmm.getSwapPoolInfo(
    poolId.toBase58(),
    A_TO_B
  );

  const bitmap = getPdaExBitmapAccount(
    CLMM_PROGRAM,
    poolId
  ).publicKey;

  const bitmapInfo =
    await connection.getAccountInfo(bitmap);

  const quote: any = swapInternal({
    programId: CLMM_PROGRAM,
    poolId,
    poolInfo: rpcData,
    tickArrays,
    configInfo,

    tickarrayBitmapExtension: bitmapInfo
      ? TickArrayBitmapExtensionLayout.decode(
          bitmapInfo.data
        )
      : undefined,

    amountSpecified: AMOUNT,
    sqrtPriceLimitX64: new BN(0),
    zeroForOne: A_TO_B,
    isBaseInput: true,
    blockTimestamp: Math.floor(Date.now() / 1000),
    includeExtraTickArrays: true,
  } as any);

  const expected = new BN(
    quote.amountCalculated.toString()
  );

  const minOut = expected
    .mul(new BN(99))
    .div(new BN(100));

  console.log("Trader:", owner.publicKey.toBase58());
  console.log("Direction:", A_TO_B ? "A -> B" : "B -> A");
  console.log("Price before:", poolInfo.price);
  console.log("Tick before:", rpcData.tickCurrent);
  console.log("Amount in:", AMOUNT.toString());
  console.log("Expected out:", expected.toString());
  console.log("Minimum out:", minOut.toString());

  if (minOut.lte(new BN(0))) {
    throw new Error("Invalid minimum output");
  }

  const { execute } = await raydium.clmm.swap({
    poolInfo,
    poolKeys,
    inputMint: new PublicKey(inputMint),
    amountIn: AMOUNT,
    amountOutMin: minOut,
    observationId: new PublicKey(
      String(rpcData.observationId)
    ),
    ownerInfo: {
      useSOLBalance: false,
    },
    remainingAccounts: quote.accounts,
    txVersion: TxVersion.V0,
    computeBudgetConfig: {
      units: 600000,
      microLamports: 1000,
    },
  } as any);

  if (process.env.SEND !== "1") {
    console.log("Quote and transaction built");
    console.log("No transaction submitted");
    return;
  }

  const { txId } = await execute({
    sendAndConfirm: true,
  });

  console.log("SUCCESS");
  console.log("Transaction:", txId);
  console.log(explorerTx(txId));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
