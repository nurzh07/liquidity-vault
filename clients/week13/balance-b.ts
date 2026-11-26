import { PublicKey } from "@solana/web3.js";
import { connection } from "./common";

async function main() {
  const account = new PublicKey(
    "373n2e7WKyebUWYFWkYFyyfoPtdr77XoFdCQ92PQH719"
  );

  const balance =
    await connection.getTokenAccountBalance(
      account
    );

  console.log(balance.value.amount);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
