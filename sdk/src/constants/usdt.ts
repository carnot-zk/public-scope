import type { SolanaNetwork } from '../types/solana';
import { normalizeSolanaNetwork } from './programs';

/** SPL USDT mint addresses */
export const USDT_MINT: Record<SolanaNetwork, string> = {
  devnet: "caw3MghdwE1FXAtGF5rhKpqtyML4GAdHxknwNcRatpH",
  mainnet: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
};

export function getUsdtMint(network: string): string {
  return USDT_MINT[normalizeSolanaNetwork(network)];
}
