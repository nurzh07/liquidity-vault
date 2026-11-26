import {
  PublicKey,
  Keypair,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  ComputeBudgetProgram,
} from "@solana/web3.js";

import {
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";

import * as anchor from "@anchor-lang/core";
import * as fs from "fs";
import * as os from "os";
import Decimal from "decimal.js";

import {
  getPdaExBitmapAccount,
} from "@raydium-io/raydium-sdk-v2";

import {
  CLMM_PROGRAM,
  connection,
  initRaydium,
  owner,
  explorerTx,
} from "./common";

Decimal.set({ precision: 80 });

const MODE = process.env.MODE ?? "status";

const MINT_A = new PublicKey(
  "9pqQ6nSjAmt2wWzmXm8wg9h7Xek1kig593NkJ3RfgrVs"
);

const MINT_B = new PublicKey(
  "BCBtQxMBex6ivJ3cMHBbXgFJyTeqWQZhDNjwq8LnLcLV"
);

const METADATA = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

const MEMO = new PublicKey(
  "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
);

const bn = (v: any) => new anchor.BN(v.toString());

async function main() {
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(owner),
    { commitment: "confirmed" }
  );

  const idl = JSON.parse(
    fs.readFileSync(
      os.homedir() +
        "/liquidity_vault/target/idl/liquidity_vault.json",
      "utf8"
    )
  );

  const program: any = new anchor.Program(
    idl,
    provider
  );

  const pda = (
    seeds: Buffer[],
    pid = program.programId
  ) => PublicKey.findProgramAddressSync(
    seeds,
    pid
  )[0];

  const vault = pda([
    Buffer.from("vault"),
    MINT_A.toBuffer(),
  ]);

  const vaultToken = pda([
    Buffer.from("vault_token"),
    vault.toBuffer(),
  ]);

  const vaultTokenB = pda([
    Buffer.from("vault_token_b"),
    vault.toBuffer(),
  ]);

  const clmmPosition = pda([
    Buffer.from("clmm_position"),
    vault.toBuffer(),
  ]);

  const balance = async (address: PublicKey) => {
    return (
      await connection.getTokenAccountBalance(address)
    ).value;
  };

  const state =
    await program.account.clmmPosition.fetchNullable(
      clmmPosition
    );

  const balA = await balance(vaultToken);
  const balB = await balance(vaultTokenB);

  console.log("Mode:", MODE);
  console.log("Program:", program.programId.toBase58());
  console.log("Vault:", vault.toBase58());
  console.log("Position PDA:", clmmPosition.toBase58());
  console.log("Token A:", balA.uiAmountString);
  console.log("Token B:", balB.uiAmountString);
  console.log("Position active:", state?.active ?? false);

  if (state) {
    console.log("Liquidity:", state.liquidity.toString());
    console.log("Lower tick:", state.tickLower);
    console.log("Upper tick:", state.tickUpper);
  }

  if (MODE === "status") return;

  if (!["open", "collect", "close", "price"].includes(MODE)) {
    throw new Error("Unknown MODE");
  }

  const poolFile = JSON.parse(
    fs.readFileSync("pool.json", "utf8")
  );

  const poolId = new PublicKey(
    (MODE === "open" || MODE === "price")
      ? poolFile.poolId
      : state?.pool
  );

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

  const { poolInfo, poolKeys, rpcPoolInfo } = pool;

  const mint0 = new PublicKey(
    String(poolInfo.mintA.address)
  );

  const mint1 = new PublicKey(
    String(poolInfo.mintB.address)
  );

  const poolVault0 = new PublicKey(
    String(poolInfo.vault?.A ?? poolKeys?.vault?.A)
  );

  const poolVault1 = new PublicKey(
    String(poolInfo.vault?.B ?? poolKeys?.vault?.B)
  );

  const spacing = Number(
    rpcPoolInfo.tickSpacing ??
    poolInfo.config.tickSpacing
  );

  const currentTick = Number(
    rpcPoolInfo.tickCurrent
  );

  if (
    !Number.isInteger(spacing) ||
    spacing <= 0 ||
    !Number.isInteger(currentTick)
  ) {
    throw new Error("Invalid tick information");
  }


  if (MODE === "price") {
    console.log("Pool price:", poolInfo.price);
    console.log("Current tick:", currentTick);

    if (state?.active) {
      const inRange =
        currentTick >= state.tickLower &&
        currentTick < state.tickUpper;

      console.log("Lower tick:", state.tickLower);
      console.log("Upper tick:", state.tickUpper);
      console.log("In range:", inRange);
    }

    return;
  }

  const bitmapExtension = getPdaExBitmapAccount(
    CLMM_PROGRAM,
    poolId
  ).publicKey;

  const i32 = (value: number) => {
    const b = Buffer.alloc(4);
    b.writeInt32BE(value);
    return b;
  };

  const arrayStart = (tick: number) =>
    Math.floor(tick / (spacing * 60)) *
    spacing * 60;

  const tickArray = (tick: number) =>
    pda([
      Buffer.from("tick_array"),
      poolId.toBuffer(),
      i32(arrayStart(tick)),
    ], CLMM_PROGRAM);

  const commonAccounts = {
    admin: owner.publicKey,
    vault,
    clmmPosition,
    vaultToken,
    vaultTokenB,
    poolState: poolId,
    poolVault0,
    poolVault1,
    mint0,
    mint1,
    bitmapExtension,
    clmmProgram: CLMM_PROGRAM,
    protocolPosition: CLMM_PROGRAM,
    tokenProgram: TOKEN_PROGRAM_ID,
    tokenProgram2022: TOKEN_2022_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  };

  let builder: any;
  let signers: Keypair[] = [];

  if (MODE === "open") {
    if (state?.active) {
      throw new Error("Position already active");
    }

    const range = Number(
      process.env.RANGE_PERCENT ?? "5"
    ) / 100;

    const lower = Math.floor(
      (
        currentTick +
        Math.log(1 - range) / Math.log(1.0001)
      ) / spacing
    ) * spacing;

    const upper = Math.ceil(
      (
        currentTick +
        Math.log(1 + range) / Math.log(1.0001)
      ) / spacing
    ) * spacing;

    const maxA = new Decimal(
      process.env.MAX_A ?? "50000000"
    );

    const maxB = new Decimal(
      process.env.MAX_B ?? "50000000"
    );

    if (
      maxA.lte(0) ||
      maxB.lte(0) ||
      maxA.gt(balA.amount) ||
      maxB.gt(balB.amount)
    ) {
      throw new Error("Insufficient vault balances");
    }

    const max0 = mint0.equals(MINT_A) ? maxA : maxB;
    const max1 = mint0.equals(MINT_A) ? maxB : maxA;

    const Q64 = new Decimal(2).pow(64);

    const sqrtPrice = new Decimal(
      rpcPoolInfo.sqrtPriceX64.toString()
    ).div(Q64);

    const sqrtLower = new Decimal("1.0001")
      .pow(new Decimal(lower).div(2));

    const sqrtUpper = new Decimal("1.0001")
      .pow(new Decimal(upper).div(2));

    const liquidity0 = max0
      .mul(sqrtPrice)
      .mul(sqrtUpper)
      .div(sqrtUpper.minus(sqrtPrice));

    const liquidity1 = max1.div(
      sqrtPrice.minus(sqrtLower)
    );

    const liquidity = Decimal.min(
      liquidity0,
      liquidity1
    ).mul("0.98").floor();

    if (!liquidity.isPositive()) {
      throw new Error("Invalid liquidity");
    }

    const nft = Keypair.generate();

    const nftAccount = getAssociatedTokenAddressSync(
      nft.publicKey,
      vault,
      true
    );

    const personalPosition = pda([
      Buffer.from("position"),
      nft.publicKey.toBuffer(),
    ], CLMM_PROGRAM);

    const metadataAccount = pda([
      Buffer.from("metadata"),
      METADATA.toBuffer(),
      nft.publicKey.toBuffer(),
    ], METADATA);

    console.log("Current tick:", currentTick);
    console.log("Lower tick:", lower);
    console.log("Upper tick:", upper);
    console.log("Liquidity:", liquidity.toFixed(0));
    console.log("NFT mint:", nft.publicKey.toBase58());

    builder = program.methods.openClmmPosition(
      bn(liquidity.toFixed(0)),
      bn(max0.toFixed(0)),
      bn(max1.toFixed(0)),
      lower,
      upper,
      arrayStart(lower),
      arrayStart(upper)
    ).accountsPartial({
      ...commonAccounts,
      nftMint: nft.publicKey,
      nftAccount,
      metadataAccount,
      personalPosition,
      tickArrayLower: tickArray(lower),
      tickArrayUpper: tickArray(upper),
      metadataProgram: METADATA,
      associatedTokenProgram:
        ASSOCIATED_TOKEN_PROGRAM_ID,
      rent: SYSVAR_RENT_PUBKEY,
    });

    signers = [nft];

  } else {
    if (!state?.active) {
      throw new Error("No active position");
    }

    const nftMint = new PublicKey(state.nftMint);

    const nftAccount = getAssociatedTokenAddressSync(
      nftMint,
      vault,
      true
    );

    const accounts = {
      ...commonAccounts,
      nftMint,
      nftAccount,
      personalPosition: state.personalPosition,
      tickArrayLower: tickArray(state.tickLower),
      tickArrayUpper: tickArray(state.tickUpper),
      memoProgram: MEMO,
    };

    if (MODE === "collect") {
      builder = program.methods
        .collectFees()
        .accountsPartial(accounts);
    }

    if (MODE === "close") {
      if (
        process.env.MIN_0 === undefined ||
        process.env.MIN_1 === undefined
      ) {
        throw new Error(
          "Set MIN_0 and MIN_1 before closing"
        );
      }

      builder = program.methods
        .closeClmmPosition(
          bn(process.env.MIN_0),
          bn(process.env.MIN_1)
        )
        .accountsPartial(accounts);
    }
  }

  builder = builder
    .preInstructions([
      ComputeBudgetProgram.setComputeUnitLimit({
        units: 1000000,
      }),
    ])
    .signers(signers);

  if (process.env.SEND !== "1") {
    const tx = await builder.transaction();

    const bh = await connection.getLatestBlockhash(
      "confirmed"
    );

    tx.feePayer = owner.publicKey;
    tx.recentBlockhash = bh.blockhash;

    tx.sign(owner, ...signers);

    const result = await connection.simulateTransaction(
      tx
    );

    for (const log of result.value.logs ?? []) {
      console.log(log);
    }

    if (result.value.err) {
      throw new Error(
        JSON.stringify(result.value.err)
      );
    }

    console.log("Simulation successful");
    console.log("No transaction submitted");

    return;
  }

  const signature = await builder.rpc();

  console.log("SUCCESS");
  console.log("Transaction:", signature);
  console.log(explorerTx(signature));

  console.log(
    "Token A after:",
    (await balance(vaultToken)).uiAmountString
  );

  console.log(
    "Token B after:",
    (await balance(vaultTokenB)).uiAmountString
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
