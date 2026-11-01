import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { LiquidityVault } from "../target/types/liquidity_vault";

const TOKEN_PROGRAM_ID = new anchor.web3.PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);
const addr = (a: anchor.web3.PublicKey) =>
  `https://explorer.solana.com/address/${a.toBase58()}?cluster=devnet`;
const tx = (s: string) =>
  `https://explorer.solana.com/tx/${s}?cluster=devnet`;

describe("liquidity_vault: week 8 skeleton (Devnet)", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const provider = anchor.getProvider() as anchor.AnchorProvider;
  const program = anchor.workspace.liquidityVault as Program<LiquidityVault>;
  const wallet = provider.wallet.publicKey;

  const depositMint = new anchor.web3.PublicKey(process.env.DEPOSIT_MINT!);
  const pda = (seeds: Buffer[]) =>
    anchor.web3.PublicKey.findProgramAddressSync(seeds, program.programId)[0];

  const vault = pda([Buffer.from("vault"), depositMint.toBuffer()]);
  const shareMint = pda([Buffer.from("share_mint"), vault.toBuffer()]);
  const vaultToken = pda([Buffer.from("vault_token"), vault.toBuffer()]);
  const position = pda([Buffer.from("position"), vault.toBuffer(), wallet.toBuffer()]);

  it("creates the vault and the user position", async () => {
    console.log("Program:      ", program.programId.toBase58());
    console.log("Admin/User:   ", wallet.toBase58());
    console.log("Deposit mint: ", depositMint.toBase58());

    // 1. initialize_vault (один раз)
    if (!(await program.account.vault.fetchNullable(vault))) {
      const sig = await program.methods
        .initializeVault()
        .accountsPartial({
          admin: wallet,
          depositMint,
          vault,
          shareMint,
          vaultToken,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      console.log("initialize_vault tx:", tx(sig));
    } else {
      console.log("Vault already exists, skipping initialize_vault");
    }

    const v = await program.account.vault.fetch(vault);
    console.log("\n--- Vault account ---");
    console.log("admin:          ", v.admin.toBase58());
    console.log("deposit_mint:   ", v.depositMint.toBase58());
    console.log("share_mint:     ", v.shareMint.toBase58());
    console.log("vault_token:    ", v.vaultToken.toBase58());
    console.log("total_deposited:", v.totalDeposited.toString());

    // 2. open_user_position (карточка пользователя)
    if (!(await program.account.userPosition.fetchNullable(position))) {
      const sig = await program.methods
        .openUserPosition()
        .accountsPartial({ user: wallet, vault, position,
          systemProgram: anchor.web3.SystemProgram.programId })
        .rpc();
      console.log("\nopen_user_position tx:", tx(sig));
    } else {
      console.log("\nPosition already exists, skipping open_user_position");
    }

    const p = await program.account.userPosition.fetch(position);
    console.log("\n--- UserPosition account ---");
    console.log("vault:    ", p.vault.toBase58());
    console.log("owner:    ", p.owner.toBase58());
    console.log("deposited:", p.deposited.toString());

    console.log("\n--- Explorer links (для README) ---");
    console.log("Program:     ", addr(program.programId));
    console.log("Vault:       ", addr(vault));
    console.log("Share mint:  ", addr(shareMint));
    console.log("Vault token: ", addr(vaultToken));
    console.log("Position:    ", addr(position));

    if (v.admin.toBase58() !== wallet.toBase58()) throw new Error("Wrong admin");
    if (p.owner.toBase58() !== wallet.toBase58()) throw new Error("Wrong owner");
  });
});
