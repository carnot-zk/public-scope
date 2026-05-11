import { execSync } from "child_process";
import dotenv from "dotenv";
import { expandHome } from "./utils";

dotenv.config();

type Env = "devnet" | "mainnet";

function parseDeployEnv(raw: string | undefined): Env {
  if (raw === "devnet" || raw === "mainnet") return raw;
  throw new Error(
    `Environment "${raw ?? ""}" is not supported. Use: devnet | mainnet`,
  );
}

function deploy() {
  const programIdx = process.argv.indexOf("--program");
  const envIdx = process.argv.indexOf("--env");

  if (programIdx === -1 || envIdx === -1) {
    throw new Error(
      "Usage: bun scripts/deploy.ts --program <name> --env <devnet|mainnet>",
    );
  }

  const program = process.argv[programIdx + 1];
  const env = parseDeployEnv(process.argv[envIdx + 1]);

  const programName = program.toLowerCase();
  const programKeypairPath = `${env === "devnet" ? process.env.DEVNET_KEYS_PATH ?? "./keys" : process.env.MAINNET_KEYS_PATH ?? "./keys"}/${programName}-keypair.json`;
  let buildCommand = `anchor build --program-name ${programName}`;
  let deployCommand: string;
  let solanaConfigCommand: string;

  if (env === "devnet") {
    const rpcUrl =
      process.env.ANCHOR_PROVIDER_DEVNET_URL ?? "https://api.devnet.solana.com";
    const wsUrl =
      process.env.ANCHOR_PROVIDER_DEVNET_WS_URL ??
      "wss://api.devnet.solana.com";
    // Build with staging feature flag for devnet
    buildCommand += " -- --features staging";
    const keypairPath = programKeypairPath;
    const walletPath = process.env.ANCHOR_WALLET
      ? ` --provider.wallet ${expandHome(process.env.ANCHOR_WALLET)}`
      : "";
    deployCommand = `anchor deploy --no-idl --program-keypair ${keypairPath} --provider.cluster ${env} --program-name ${programName}${walletPath}`;
    solanaConfigCommand = `solana config set --url ${rpcUrl} --ws ${wsUrl}`;
  } else {
    const rpcUrl =
      process.env.ANCHOR_PROVIDER_MAINNET_URL ??
      "https://api.mainnet-beta.solana.com";
    const wsUrl =
      process.env.ANCHOR_PROVIDER_MAINNET_WS_URL ??
      "wss://api.mainnet-beta.solana.com";
    const keypairPath = programKeypairPath;
    const walletPath = process.env.ANCHOR_WALLET
      ? ` --provider.wallet ${expandHome(process.env.ANCHOR_WALLET)}`
      : "";
    deployCommand = `anchor deploy --no-idl --program-keypair ${keypairPath} --provider.cluster ${env} --program-name ${programName}${walletPath}`;
    solanaConfigCommand = `solana config set --url ${rpcUrl} --ws ${wsUrl}`;
  }

  try {
    console.log(`\nSetting Solana config for ${env}...\n`);
    execSync(solanaConfigCommand, { stdio: "inherit" });

    console.log(`\nBuilding ${programName} for ${env}...\n`);
    execSync(buildCommand, { stdio: "inherit" });

    console.log(`\nDeploying ${programName} to ${env}...\n`);
    execSync(deployCommand, { stdio: "inherit" });

    const programId = execSync(`solana address -k ${programKeypairPath}`, {
      encoding: "utf8",
    }).trim();
    const idlPath = `target/idl/${programName}.json`;
    console.log(`\nUploading IDL for ${programName} (${programId})...\n`);
    try {
      execSync(
        `anchor idl init --filepath ${idlPath} ${programId} --provider.cluster ${env}${process.env.ANCHOR_WALLET ? ` --provider.wallet ${expandHome(process.env.ANCHOR_WALLET)}` : ""}`,
        { stdio: "inherit" },
      );
    } catch {
      execSync(
        `anchor idl upgrade --filepath ${idlPath} ${programId} --provider.cluster ${env}${process.env.ANCHOR_WALLET ? ` --provider.wallet ${expandHome(process.env.ANCHOR_WALLET)}` : ""}`,
        { stdio: "inherit" },
      );
    }

    console.log(`\nSuccessfully deployed ${programName} to ${env}`);
  } catch (error) {
    console.error("\nDeployment failed:", error);
    process.exit(1);
  }
}

deploy();
