import { PublicKey, SystemProgram, ComputeBudgetProgram } from "@solana/web3.js";
import * as anchor from "@anchor-lang/core";
import * as fs from "fs";
import * as os from "os";
import {
  swapInternal,
  getPdaExBitmapAccount,
  TickArrayBitmapExtensionLayout,
} from "@raydium-io/raydium-sdk-v2";
import { CLMM_PROGRAM, connection, initRaydium, owner, explorerTx } from "./common";

const MODE = process.env.MODE ?? "swap"; // init | swap | swap-bad
const AMOUNT_IN = new anchor.BN(process.env.AMOUNT_IN ?? "100000000"); // 100 токенов
const SLIPPAGE_BPS = Number(process.env.SLIPPAGE_BPS ?? "100");        // 1%
const A_TO_B = (process.env.DIRECTION ?? "a2b") === "a2b";

const TOKEN_PROGRAM = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const TOKEN_2022 = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const MEMO = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const DEPOSIT_MINT = new PublicKey("9pqQ6nSjAmt2wWzmXm8wg9h7Xek1kig593NkJ3RfgrVs"); // токен A
const TOKEN_B_MINT = new PublicKey("BCBtQxMBex6ivJ3cMHBbXgFJyTeqWQZhDNjwq8LnLcLV"); // токен B

async function main() {
  const wallet = new anchor.Wallet(owner);
  const provider = new anchor.AnchorProvider(connection, wallet, { commitment: "confirmed" });
  const idl = JSON.parse(
    fs.readFileSync(os.homedir() + "/liquidity_vault/target/idl/liquidity_vault.json", "utf8")
  );
  const program: any = new anchor.Program(idl, provider);

  const pda = (seeds: Buffer[]) => PublicKey.findProgramAddressSync(seeds, program.programId)[0];
  const vault = pda([Buffer.from("vault"), DEPOSIT_MINT.toBuffer()]);
  const vaultToken = pda([Buffer.from("vault_token"), vault.toBuffer()]);
  const vaultTokenB = pda([Buffer.from("vault_token_b"), vault.toBuffer()]);

  const bal = async (a: PublicKey) => {
    try { return (await connection.getTokenAccountBalance(a)).value.uiAmountString; }
    catch { return "(нет)"; }
  };

  console.log("Program:", program.programId.toBase58());
  console.log("Admin:  ", owner.publicKey.toBase58());
  console.log("Vault:  ", vault.toBase58());
  console.log("Vault token A:", vaultToken.toBase58(), "| баланс", await bal(vaultToken));
  console.log("Vault token B:", vaultTokenB.toBase58(), "| баланс", await bal(vaultTokenB));

  // Отправка с получением подписи даже для провальной транзакции
  const send = async (builder: any, skipPreflight: boolean) => {
    const tx = await builder.transaction();
    const bh = await connection.getLatestBlockhash("confirmed");
    tx.feePayer = owner.publicKey;
    tx.recentBlockhash = bh.blockhash;
    tx.sign(owner);
    const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight });
    await connection.confirmTransaction(
      { signature: sig, blockhash: bh.blockhash, lastValidBlockHeight: bh.lastValidBlockHeight },
      "confirmed"
    );
    const st = await connection.getSignatureStatus(sig, { searchTransactionHistory: true });
    return { sig, err: st.value?.err ?? null };
  };

  if (MODE === "init") {
    const builder = program.methods.initTokenB().accountsPartial({
      admin: owner.publicKey, vault, tokenBMint: TOKEN_B_MINT, vaultTokenB,
      tokenProgram: TOKEN_PROGRAM, systemProgram: SystemProgram.programId,
    });
    const r = await send(builder, false);
    console.log("\ninit_token_b:", r.err ? "ОШИБКА " + JSON.stringify(r.err) : "OK", explorerTx(r.sig));
    return;
  }

  // ---- swap / swap-bad ----
  const info = JSON.parse(fs.readFileSync("pool.json", "utf8"));
  const raydium = await initRaydium();
  const getPool = async () => {
    const orig = Object.values;
    (Object as any).values = (o: any) => (o == null ? [] : orig(o));
    try { return await raydium.clmm.getPoolInfoFromRpc(info.poolId); }
    finally { (Object as any).values = orig; }
  };
  const before: any = await getPool();
  const { poolInfo, poolKeys } = before;
  console.log("\nЦена пула ДО:  ", poolInfo.price, "| тик", before.rpcPoolInfo?.tickCurrent);

  // В нашем пуле mintA = токен A (депозитный), mintB = токен B
  const zeroForOne = A_TO_B;
  const poolIdPub = new PublicKey(info.poolId);
  const { rpcData, tickArrays, configInfo } = await raydium.clmm.getSwapPoolInfo(info.poolId, zeroForOne);
  const exBitmap = getPdaExBitmapAccount(CLMM_PROGRAM, poolIdPub).publicKey;
  const exInfo = await connection.getAccountInfo(exBitmap);

  const sim: any = swapInternal({
    programId: CLMM_PROGRAM, poolId: poolIdPub, poolInfo: rpcData, tickArrays, configInfo,
    tickarrayBitmapExtension: exInfo ? TickArrayBitmapExtensionLayout.decode(exInfo.data) : undefined,
    amountSpecified: new (require("bn.js"))(AMOUNT_IN.toString()),
    sqrtPriceLimitX64: new (require("bn.js"))(0),
    zeroForOne, isBaseInput: true,
    blockTimestamp: Math.floor(Date.now() / 1000), includeExtraTickArrays: true,
  } as any);
  const expected = new anchor.BN(sim.amountCalculated.toString());
  if (sim.accounts.length !== 2) throw new Error("Ожидалось 2 tick-массива, получено " + sim.accounts.length);

  const minOut = MODE === "swap-bad"
    ? expected.mul(new anchor.BN(105)).div(new anchor.BN(100))          // заведомо больше, чем даст пул
    : expected.mul(new anchor.BN(10000 - SLIPPAGE_BPS)).div(new anchor.BN(10000));
  console.log("Направление:     ", A_TO_B ? "A -> B" : "B -> A");
  console.log("Отдаём (base):   ", AMOUNT_IN.toString());
  console.log("Ожидаем (base):  ", expected.toString());
  console.log("Минимум (base):  ", minOut.toString(), MODE === "swap-bad" ? "(заведомо завышен)" : `(slippage ${SLIPPAGE_BPS / 100}%)`);

  const poolVaultA = poolInfo.vault?.A ?? poolKeys?.vault?.A;
  const poolVaultB = poolInfo.vault?.B ?? poolKeys?.vault?.B;
  const builder = program.methods
    .swap(AMOUNT_IN, minOut, new anchor.BN(0), A_TO_B)
    .accountsPartial({
      admin: owner.publicKey, vault,
      depositMint: DEPOSIT_MINT, tokenBMint: TOKEN_B_MINT,
      vaultToken, vaultTokenB,
      clmmProgram: CLMM_PROGRAM,
      ammConfig: new PublicKey(String(poolInfo.config.id)),
      poolState: poolIdPub,
      poolInputVault: new PublicKey(String(A_TO_B ? poolVaultA : poolVaultB)),
      poolOutputVault: new PublicKey(String(A_TO_B ? poolVaultB : poolVaultA)),
      observationState: new PublicKey(String(rpcData.observationId)),
      tokenProgram2022: TOKEN_2022, memoProgram: MEMO,
      bitmapExtension: exBitmap,
      tickArray0: new PublicKey(String(sim.accounts[0])),
      tickArray1: new PublicKey(String(sim.accounts[1])),
      tokenProgram: TOKEN_PROGRAM,
    })
    .preInstructions([ComputeBudgetProgram.setComputeUnitLimit({ units: 400000 })]);

  const r = await send(builder, MODE === "swap-bad");
  console.log("\n=== Результат ===");
  console.log("Статус: ", r.err ? "ОТКЛОНЕНА: " + JSON.stringify(r.err) : "УСПЕХ");
  console.log("Tx:     ", explorerTx(r.sig));

  await new Promise((x) => setTimeout(x, 3000));
  const after: any = await getPool();
  console.log("\nЦена пула ПОСЛЕ:", after.poolInfo.price, "| тик", after.rpcPoolInfo?.tickCurrent);
  console.log("Vault token A:", await bal(vaultToken), "| Vault token B:", await bal(vaultTokenB));
}
main().catch((e) => { console.error(e); process.exit(1); });
