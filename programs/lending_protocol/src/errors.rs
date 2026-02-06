use anchor_lang::prelude::*;

//Error Codes
#[error_code]
pub enum AuthorizationError 
{
    #[msg("Only the CEO can call this function")]
    NotCEO,
    #[msg("Only the Solvency Treasurer can call this function")]
    NotSolvencyTreasurer,
    #[msg("Only the Liquidation Treasurer can call this function")]
    NotLiquidationTreasurer,
    #[msg("Only the Fee Collector can claim the fees")]
    NotFeeCollector
}

#[error_code]
pub enum InvalidInputError
{
    #[msg("The submarket fee on interest earned rate can't be greater than 100%")]
    InvalidSubMarketFeeRate,
    #[msg("The solvency insurance fee on interest earned rate can't be greater than 100%")]
    InvalidSolvencyInsuranceFeeRate,
    #[msg("You must provide all of the sub user's tab accounts")]
    IncorrectNumberOfTabAccounts,
    #[msg("You must provide all of the sub user's tab accounts and Pyth price update accounts")]
    IncorrectNumberOfTabAndPythPriceUpdateAccounts,
    #[msg("You must provide the sub user's tab accounts ordered by user_tab_account_index")]
    IncorrectOrderOfTabAccounts,
    #[msg("Unexpected Lending Stats PDA detected. Feed in only legitimate PDA's ordered by user_tab_account_index")]
    UnexpectedLendingStatsAccount,
    #[msg("Unexpected Tab Account PDA detected. Feed in only legitimate PDA's ordered by user_tab_account_index")]
    UnexpectedTabAccount,
    #[msg("Unexpected Pyth Price Update Account detected. Feed in only legitimate accounts :)")]
    UnexpectedPythPriceUpdateAccount,
    #[msg("Unexpected Token Reserve Account PDA detected")]
    UnexpectedTokenReserveAccount,
    #[msg("Unexpected SubMarket Account PDA detected")]
    UnexpectedSubMarketAccount,
    #[msg("Unexpected Monthly Statement Account PDA detected")]
    UnexpectedMonthlyStatementAccount,
    #[msg("Lending User Account name can't be longer than 25 characters")]
    LendingUserAccountNameTooLong,
    #[msg("You can't deposit more than the global limit")]
    GlobalLimitExceeded
}

#[error_code]
pub enum LendingError
{
    #[msg("You can't withdraw more funds than you've deposited")]
    InsufficientFunds,
    #[msg("Not enough liquidity in the Token Reserve for this withdraw or borrow")]
    InsufficientLiquidity,
    #[msg("You can't pay back more funds than you've borrowed")]
    TooManyFunds,
    #[msg("The token reserve was stale")]
    StaleTokenReserve,
    #[msg("The lending user health data was stale")]
    StaleLendingUser,
    #[msg("The price data was stale or the feed id was incorrect")]
    StalePriceDataOrWrongFeedID,
    #[msg("You must repay atleast 10% of the borrow position if the account is in an unhealthy state. This prevents 'griefing'")]
    GriefingRepayment,
    #[msg("You can't withdraw or borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral")]
    LiquidationExposure,
    #[msg("You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral")]
    NotLiquidatable,
    #[msg("You can't repay more than 50% of a liquidati's debt position")]
    OverLiquidation,
    #[msg("You can't zero out an account whose borrow liabilities aren't 100% or more of their deposited collateral")]
    NotInsolvent,
    #[msg("Duplicate SubMarket Detected")]
    DuplicateSubMarket,
    #[msg("Negative Price Detected")]
    NegativePriceDetected,
    #[msg("Oracle Price Too Unstable")]
    OraclePriceTooUnstable
}