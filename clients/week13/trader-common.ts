import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { Raydium, TxVersion } from "@raydium-io/raydium-sdk-v2";
import * as fs from "fs";
import * as os from "os";

// CLMM-программа Raydium в DEVNET (официальная таблица адресов Raydium)
export const CLMM_PROGRAM = new PublicKey("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH");
export const TOKEN_PROGRAM = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

export const connection = new Connection("https://api.devnet.solana.com", "confirmed");
export const owner = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync(os.homedir() + "/.config/solana/week13-trader.json", "utf8")))
);
export const txVersion = TxVersion.V0;

export const initRaydium = async () =>
  Raydium.load({
    owner,
    connection,
    cluster: "devnet",          // важно: devnet, а не mainnet
    disableFeatureCheck: true,
    disableLoadToken: true,     // свои токены, список токенов Raydium не нужен
    blockhashCommitment: "finalized",
  });

// Описание своего токена (в devnet задаём его вручную, API токена не знает)
export const tokenInfo = (address: string, decimals = 6): any => ({
  chainId: 103,
  address,
  programId: TOKEN_PROGRAM,
  logoURI: "",
  symbol: "",
  name: "",
  decimals,
  tags: [],
  extensions: {},
});

export const explorerTx = (sig: string) =>
  `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
export const explorerAddr = (a: string) =>
  `https://explorer.solana.com/address/${a}?cluster=devnet`;

// Удобная печать объектов с PublicKey и BN
export const show = (o: any) =>
  console.log(
    JSON.stringify(o, (_k, v) =>
      v && v.constructor && v.constructor.name === "PublicKey" ? v.toBase58()
      : v && v.constructor && v.constructor.name === "BN" ? v.toString()
      : v, 2)
  );
