import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { LiquidityVault } from "../target/types/liquidity_vault";

const { PublicKey, SystemProgram, Transaction, TransactionInstruction } = anchor.web3;
type PK = anchor.web3.PublicKey;
const BN = anchor.BN;

const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const tx = (s: string) => `https://explorer.solana.com/tx/${s}?cluster=devnet`;
const ata = (mint: PK, owner: PK) =>
  PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ATA_PROGRAM_ID
  )[0];
const tokens = (n: number) => new BN(n).mul(new BN(1_000_000)); // 6 decimals

describe("liquidity_vault: week 9 deposit (Devnet)", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const provider = anchor.getProvider() as anchor.AnchorProvider;
  const conn = provider.connection;
  const program = anchor.workspace.liquidityVault as Program<LiquidityVault>;
  const wallet = provider.wallet.publicKey;

  const depositMint = new PublicKey(process.env.DEPOSIT_MINT!);
  const pda = (seeds: Buffer[]) =>
    PublicKey.findProgramAddressSync(seeds, program.programId)[0];
  const vault = pda([Buffer.from("vault"), depositMint.toBuffer()]);
  const shareMint = pda([Buffer.from("share_mint"), vault.toBuffer()]);
  const vaultToken = pda([Buffer.from("vault_token"), vault.toBuffer()]);
  const position = pda([Buffer.from("position"), vault.toBuffer(), wallet.toBuffer()]);
  const userToken = ata(depositMint, wallet);
  const userShare = ata(shareMint, wallet);

  const accts = {
    user: wallet, vault, depositMint, shareMint, vaultToken, userToken, userShare,
    position, tokenProgram: TOKEN_PROGRAM_ID, associatedTokenProgram: ATA_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  };

  const bal = async (a: PK) => {
    try { return new BN((await conn.getTokenAccountBalance(a)).value.amount); }
    catch { return new BN(0); }
  };
  const supply = async () => new BN((await conn.getTokenSupply(shareMint)).value.amount);
  const fmt = (x: any) => new BN(x).toString();

  async function deposit(amount: any, label: string) {
    const s = await supply();
    const assets = await bal(vaultToken);
    const expected = s.isZero() ? amount : amount.mul(s).div(assets);
    const before = await bal(userShare);

    const sig = await program.methods.deposit(amount).accountsPartial(accts).rpc();

    const got = (await bal(userShare)).sub(before);
    console.log(`\n=== ${label} ===`);
    console.log("Deposit tx:         ", tx(sig));
    console.log("Внесено (base):     ", fmt(amount));
    console.log("Vault до: активы    ", fmt(assets), " / share-токенов", fmt(s));
    console.log("Ожидаемо share:     ", fmt(expected));
    console.log("Получено share:     ", fmt(got));
    if (!got.eq(expected)) throw new Error("Shares mismatch");
  }

  it("rejects zero deposit, deposits and keeps fair shares", async () => {
    // 1. Нулевой депозит запрещён
    let rejected = false;
    try {
      await program.methods.deposit(new BN(0)).accountsPartial(accts).rpc();
    } catch (e) { const m = String(e); rejected = m.includes("ZeroAmount") || m.includes("greater than zero"); console.log("Zero deposit error:", m.split("\n")[0]); }
    if (!rejected) throw new Error("Zero deposit was NOT rejected");

    // 2. Депозит 1000 токенов
    await deposit(tokens(1000), "Депозит 1: 1000 токенов");

    // 3. Имитация дохода vault: 100 токенов напрямую в хранилище
    //    (настоящий доход появится позже, когда vault начнёт работать в пуле)
    const data = Buffer.alloc(9);
    data.writeUInt8(3, 0); // SPL Token: Transfer
    data.writeBigUInt64LE(BigInt(tokens(100).toString()), 1);
    const ix = new TransactionInstruction({
      programId: TOKEN_PROGRAM_ID,
      keys: [
        { pubkey: userToken, isSigner: false, isWritable: true },
        { pubkey: vaultToken, isSigner: false, isWritable: true },
        { pubkey: wallet, isSigner: true, isWritable: false },
      ],
      data,
    });
    const profitSig = await provider.sendAndConfirm(new Transaction().add(ix));
    console.log("\nИмитация дохода (+100 токенов в vault):", tx(profitSig));

    // 4. Поздний вкладчик получает меньше share-токенов, чем внёс токенов
    await deposit(tokens(110), "Депозит 2: 110 токенов (после дохода)");

    // 5. Итоговое состояние
    const v = await program.account.vault.fetch(vault);
    const p = await program.account.userPosition.fetch(position);
    console.log("\n--- Итог ---");
    console.log("vault.total_deposited:    ", v.totalDeposited.toString());
    console.log("position.deposited:       ", p.deposited.toString());
    console.log("Share-токены пользователя:", fmt(await bal(userShare)));
    console.log("Токены в хранилище:       ", fmt(await bal(vaultToken)));
    console.log("Выпущено share-токенов:   ", fmt(await supply()));
  });
});
