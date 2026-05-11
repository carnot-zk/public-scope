import type { ProgramIds, SolanaNetwork } from '../types/solana';

export function normalizeSolanaNetwork(network: string): SolanaNetwork {
  if (network === 'mainnet-beta' || network === 'mainnet') return 'mainnet';
  return 'devnet';
}

export const PROGRAM_IDS: Record<SolanaNetwork, ProgramIds> = {
  devnet: {
    carnot: 'cartX31ocscytAK988e5h1xAMAeNXt6zqdgeyr3pZ3U',
  },
  mainnet: {
    carnot: "carCrmy6qN8tRgvUp9v6JrfUuxroGrKdndUdwMMNumS"
  },
};
