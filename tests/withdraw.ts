import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { LiquidityVault } from "../target/types/liquidity_vault";

const { PublicKey } = anchor.web3;
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

describe("liquidity_vault: week 10 withdraw (Devnet)", () => {
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
    position, tokenProgram: TOKEN_PROGRAM_ID,
  };

  const bal = async (a: PK) => {
    try { return new BN((await conn.getTokenAccountBalance(a)).value.amount); }
    catch { return new BN(0); }
  };
  const supply = async () => new BN((await conn.getTokenSupply(shareMint)).value.amount);
  const fmt = (x: any) => new BN(x).toString();

  const expectError = async (p: Promise<any>, code: string) => {
    try { await p; }
    catch (e) {
      if (String(e).includes(code)) { console.log("Rejected as expected:", code); return; }
      throw e;
    }
    throw new Error("Expected error " + code);
  };

  it("burns shares and returns the proportional amount of tokens", async () => {
    // 1. Нулевой вывод и вывод больше, чем есть share-токенов, запрещены
    await expectError(program.methods.withdraw(new BN(0)).accountsPartial(accts).rpc(), "ZeroAmount");
    const owned = await bal(userShare);
    await expectError(
      program.methods.withdraw(owned.add(new BN(1))).accountsPartial(accts).rpc(),
      "InsufficientShares"
    );

    // 2. Выводим половину своих share-токенов
    const shares = owned.div(new BN(2));
    const s = await supply();
    const assets = await bal(vaultToken);
    const expected = shares.mul(assets).div(s);
    const tokBefore = await bal(userToken);
    const posBefore = await program.account.userPosition.fetch(position);
    const vaultBefore = await program.account.vault.fetch(vault);

    const sig = await program.methods.withdraw(shares).accountsPartial(accts).rpc();

    const tokGot = (await bal(userToken)).sub(tokBefore);
    const sAfter = await supply();
    const assetsAfter = await bal(vaultToken);
    const posAfter = await program.account.userPosition.fetch(position);
    const vaultAfter = await program.account.vault.fetch(vault);

    console.log("\n=== Вывод ===");
    console.log("Withdraw tx:             ", tx(sig));
    console.log("Возвращено share (base): ", fmt(shares));
    console.log("Vault до:  токены", fmt(assets), "/ share", fmt(s));
    console.log("Vault после: токены", fmt(assetsAfter), "/ share", fmt(sAfter));
    console.log("Ожидаемо токенов:        ", fmt(expected));
    console.log("Получено токенов:        ", fmt(tokGot));
    console.log("position.deposited:      ", posBefore.deposited.toString(), "->", posAfter.deposited.toString());
    console.log("vault.total_deposited:   ", vaultBefore.totalDeposited.toString(), "->", vaultAfter.totalDeposited.toString());
    console.log("Цена share до/после:     ",
      (Number(fmt(assets)) / Number(fmt(s))).toFixed(4), "/",
      (Number(fmt(assetsAfter)) / Number(fmt(sAfter))).toFixed(4));

    if (!tokGot.eq(expected)) throw new Error("Withdrawn amount mismatch");
    if (!s.sub(sAfter).eq(shares)) throw new Error("Shares were not burned");
  });
});
