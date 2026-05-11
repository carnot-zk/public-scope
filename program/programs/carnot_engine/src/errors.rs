use anchor_lang::prelude::*;

#[error_code]
pub enum CarnotError {
    #[msg(INVALID_ZK_PROOF)]
    InvalidProof,

    #[msg(BATCH_SUBMITTED_TOO_SOON)]
    BatchTooSoon,

    #[msg(PROGRAM_IS_PAUSED)]
    Paused,

    #[msg(INSUFFICIENT_SOLVENCY_RATIO)]
    InsufficientSolvency,

    #[msg(INSUFFICIENT_LP_SHARES)]
    InsufficientShares,

    #[msg(WITHDRAWAL_AMOUNT_EXCEEDS_WITHDRAWABLE_BALANCE)]
    ExceedsWithdrawableBalance,

    #[msg(UNAUTHORIZED)]
    Unauthorized,

    #[msg(INVALID_MARKET_REGIME)]
    InvalidMarketRegime,

    #[msg(INVALID_AMOUNT)]
    InvalidAmount,

    #[msg(LP_WITHDRAWAL_TIMELOCK_NOT_EXPIRED)]
    TimelockNotExpired,

    #[msg(INSUFFICIENT_AVAILABLE_TRADER_MARGIN)]
    InsufficientAvailableMargin,

    #[msg(INVALID_POOL_BALANCE_COMMITMENT)]
    InvalidPoolBalance,

    #[msg(INVALID_ORACLE_PRICE_HASH)]
    InvalidPriceHash,

    #[msg(INVALID_GROTH16_VERIFIER_INPUT)]
    InvalidGroth16Input,

    #[msg(INVALID_ORACLE_ACCOUNT)]
    InvalidOracleAccount,

    #[msg(ARITHMETIC_OVERFLOW)]
    ArithmeticOverflow,

    #[msg(BATCH_HAS_NOT_SETTLED_YET)]
    BatchNotSettled,

    #[msg(BATCH_ID_DOES_NOT_MATCH_TRADE_BATCH)]
    InvalidBatch,

    #[msg(INVALID_MERKLE_PROOF_FOR_PAYOUT_CLAIM)]
    InvalidMerkleProof,
}
