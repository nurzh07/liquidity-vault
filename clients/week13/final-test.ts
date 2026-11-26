import * as anchor from "@anchor-lang/core";
import {
  PublicKey,
  Keypair,
  SystemProgram,
} from "@solana/web3.js";

import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";

import * as fs from "fs";
import * as os from "os";

import {
  connection,
  owner,
} from "./common";

const idl = JSON.parse(
  fs.readFileSync(
    os.homedir() +
    "/liquidity_vault/target/idl/liquidity_vault.json",
    "utf8"
  )
);

const provider = new anchor.AnchorProvider(
  connection,
  new anchor.Wallet(owner),
  { commitment: "confirmed" }
);

const program: any = new anchor.Program(
  idl,
  provider
);

const MINT_A = new PublicKey(
  "9pqQ6nSjAmt2wWzmXm8wg9h7Xek1kig593NkJ3RfgrVs"
);

const trader = Keypair.fromSecretKey(
  Uint8Array.from(
    JSON.parse(
      fs.readFileSync(
        os.homedir() +
        "/.config/solana/week13-trader.json",
        "utf8"
      )
    )
  )
);

function pda(...seeds: Buffer[]) {
  return PublicKey.findProgramAddressSync(
    seeds,
    program.programId
  )[0];
}

const vault = pda(
  Buffer.from("vault"),
  MINT_A.toBuffer()
);

const clmmPosition = pda(
  Buffer.from("clmm_position"),
  vault.toBuffer()
);

const vaultTokenB = pda(
  Buffer.from("vault_token_b"),
  vault.toBuffer()
);

async function simulate(
  builder: any,
  signer: Keypair
) {
  const tx = await builder.transaction();

  const bh =
    await connection.getLatestBlockhash();

  tx.feePayer = signer.publicKey;
  tx.recentBlockhash = bh.blockhash;

  tx.sign(signer);

  return (
    await connection.simulateTransaction(tx)
  ).value;
}

async function expectRejected(
  name: string,
  builder: any,
  signer: Keypair,
  expected: string
) {
  const result = await simulate(
    builder,
    signer
  );

  const logs = (result.logs ?? []).join("\n");

  if (!result.err || !logs.includes(expected)) {
    console.log(logs);
    throw new Error(
      `${name}: expected ${expected}, got ${
        JSON.stringify(result.err)
      }`
    );
  }

  console.log("PASS:", name);
}

async function main() {
  const v = await program.account.vault.fetch(vault);

  const state =
    await program.account.clmmPosition.fetch(
      clmmPosition
    );

  const tokenB =
    await connection.getTokenAccountBalance(
      vaultTokenB
    );

  console.log("Vault:", vault.toBase58());
  console.log("Position active:", state.active);
  console.log("Token B:", tokenB.value.uiAmountString);

  const position = pda(
    Buffer.from("position"),
    vault.toBuffer(),
    owner.publicKey.toBuffer()
  );

  const userToken =
    getAssociatedTokenAddressSync(
      MINT_A,
      owner.publicKey
    );

  const userShare =
    getAssociatedTokenAddressSync(
      v.shareMint,
      owner.publicKey
    );

  const accounts = {
    user: owner.publicKey,
    vault,
    depositMint: MINT_A,
    shareMint: v.shareMint,
    vaultToken: v.vaultToken,
    userToken,
    userShare,
    position,
    clmmPosition,
    vaultTokenB,
    tokenProgram: TOKEN_PROGRAM_ID,
  };

  const deposit = program.methods
    .deposit(new anchor.BN(1000000))
    .accountsPartial({
      ...accounts,
      associatedTokenProgram:
        ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    });

  const withdraw = program.methods
    .withdraw(new anchor.BN(1000000))
    .accountsPartial(accounts);

  if (state.active) {
    await expectRejected(
      "Deposit blocked during CLMM",
      deposit,
      owner,
      "ClmmPositionActive"
    );

    await expectRejected(
      "Withdraw blocked during CLMM",
      withdraw,
      owner,
      "ClmmPositionActive"
    );
  } else if (tokenB.value.amount !== "0") {
    await expectRejected(
      "Deposit blocked while Token B exists",
      deposit,
      owner,
      "TokenBNotEmpty"
    );

    await expectRejected(
      "Withdraw blocked while Token B exists",
      withdraw,
      owner,
      "TokenBNotEmpty"
    );
  } else {
    const result = await simulate(
      withdraw,
      owner
    );

    console.log(result.logs);

    if (result.err) {
      throw new Error(
        JSON.stringify(result.err)
      );
    }

    console.log("PASS: Withdraw simulation");

    if (process.env.SEND === "1") {
      const signature = await withdraw.rpc();

      console.log("PASS: Real withdrawal");
      console.log("Transaction:", signature);
    }
  }

  console.log("Vault admin:", v.admin.toBase58());
  console.log("Trader:", trader.publicKey.toBase58());
  console.log("USER GUARD TESTS PASSED");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
