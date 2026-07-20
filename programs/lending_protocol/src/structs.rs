use anchor_lang::prelude::*;

//Internal Structs
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct PriceDataPayload
{
    pub data: Vec<VerifiedPriceData>,
    pub slot: u64
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct VerifiedPriceData
{
    pub token_id: u8,
    pub normalized_price_18_decimals: u128
}

//Accounts
#[account]
pub struct LendingProtocolCEO
{
    pub address: Pubkey
}

#[account]
pub struct SolvencyTreasurer
{
    pub address: Pubkey
}

#[account]
pub struct LiquidationTreasurer
{
    pub address: Pubkey
}

#[account]
pub struct OraclePriceValidator
{
    pub bump: u8,
    pub address: Pubkey
}

#[account]
pub struct TempOraclePriceAccount
{
    pub bump: u8,
    pub data: Vec<VerifiedPriceData>,
    pub slot: u64
}

#[account]
pub struct LendingProtocol
{
    pub current_statement_month: u8,
    pub current_statement_year: u16,
    pub max_tabs_per_lending_account: u8,
    pub look_up_table_address: Pubkey
}

#[account]
pub struct TokenReserveStats
{
    pub token_reserve_count: u8,
    pub token_reserves_updated_count: u32
}

#[account]
pub struct SubMarketStats //Moved these lending protocol variables here to help stream line the listeners on the front end, so that when ever there is any change what so ever on this account, we can be sure that we need to do a .all() for the SubMarket accounts on the front end without having to fetch some other account to check a different number before hand. Less fetches/alls, the better.
{
    pub sub_market_creation_count: u32,
    pub sub_market_edit_count: u32
}

#[account]
pub struct LendingStats
{
    pub bump: u8,
    pub deposits: u128,
    pub withdrawals: u128,
    pub borrows: u128,
    pub repayments: u128,
    pub liquidations: u128,
    pub snap_shots: u128,
    pub fee_collections: u128
}

#[account]
pub struct LendingUserStats
{
    pub name_change_count: u128
}

#[account]
pub struct TokenReserve
{
    pub bump: u8,
    pub token_id: u8,
    pub token_mint_address: Pubkey,
    pub token_decimal_amount: u8,
    pub supply_apy: u16,
    pub borrow_apy: u16,
    pub base_borrow_apy: u16,
    pub use_fixed_borrow_apy: bool,
    pub utilization_rate: u16,
    pub global_limit: u128,
    pub supply_interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Supply User interest is earned from Borrow Users so that it can be proportionally distributed to Supply Users
    pub borrow_interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Borrow User interest is accrued for Supply Users so that it can be proportionally distributed to Borrow Users
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub solvency_insurance_fee_rate: u16,
    pub uncollected_solvency_insurance_fees_amount: u128,
    pub uncollected_liquidation_fees_amount: u128,
    pub borrowed_amount: u128,
    pub interest_accrued_amount: u128,
    pub repaid_debt_amount: u128,
    pub liquidated_amount: u128,
    pub last_lending_activity_amount: u64,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64,
    pub last_health_update_clock_slot: u64
}

#[account]
pub struct SubMarket
{
    pub bump: u8,
    pub id: u32,
    pub owner: Pubkey,
    pub token_id: u8,
    pub sub_market_index: u16,
    pub fee_collector_address: Pubkey,
    pub fee_on_interest_earned_rate: u16,
    pub supply_interest_change_index: u128, //This index is set to match the token reserve index after previously earned interest is updated. This is only used in the frontend for calculating the 7 day projection rate
    pub borrow_interest_change_index: u128, //This index is set to match the token reserve index after previously accured interest is updated. This is only used in the frontend for calculating the 7 day projection rate
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub sub_market_fees_generated_amount: u128, //These generated fees aren't combined into one so other developers that want to use their own submarket and keep track of it separately
    pub uncollected_sub_market_fees_amount: u128,  
    pub solvency_insurance_fees_generated_amount: u128,
    pub liquidation_fees_generated_amount: u128,
    pub borrowed_amount: u128,
    pub interest_accrued_amount: u128,
    pub repaid_debt_amount: u128,
    pub liquidated_amount: u128,
    pub last_lending_activity_amount: u64,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64
}

#[account]
pub struct SubMarketOwnerLookUpTable
{
    pub owner: Pubkey,
    pub look_up_table_address: Pubkey,
    pub look_up_table_added: bool
}

#[account]
pub struct LendingUserAccount
{
    pub bump: u8,
    pub owner: Pubkey,
    pub user_account_index: u8, //Giving the lending account an index to allow users to have multiple lending accounts if they so choose, so they don't have to use multiple wallets
    pub account_name: String,
    pub lending_user_account_added: bool,
    pub tab_account_count: u8,
    pub total_deposited_usd_value: u128,
    pub total_borrowed_usd_value: u128,
    pub refresh_clock_slot: u64,
    pub last_health_update_clock_slot: u64,
    pub temp_deposit_usd_value: u128,
    pub temp_borrow_usd_value: u128,
    pub next_tab_index_to_refresh: u8,
    pub look_up_table_address: Pubkey
}

#[account]
pub struct LendingUserTabAccount
{
    pub bump: u8,
    pub token_id: u8,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub user_tab_account_index: u8,
    pub user_tab_account_added: bool,
    pub supply_interest_change_index: u128, //This index is set to match the token reserve index after previously earned interest is updated
    pub borrow_interest_change_index: u128, //This index is set to match the token reserve index after previously accured interest is updated
    pub deposited_amount: u64,
    pub interest_earned_amount: u64,
    pub fees_generated_amount: u64,
    pub borrowed_amount: u64,
    pub interest_accrued_amount: u64,
    pub repaid_debt_amount: u64,
    pub liquidated_amount: u64,
    pub liquidator_amount: u64,
    pub interest_change_last_updated_clock_slot: u64
}

#[account]
pub struct LendingUserMonthlyStatementAccount
{
    pub bump: u8,
    pub token_id: u8,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub statement_month: u8,
    pub statement_year: u16,
    pub monthly_statement_account_added: bool,
    pub snap_shot_balance_amount: u64,//The snap_shot properties give a snapshot of the value of the Tab Account over its whole life time at the time it is updated
    pub snap_shot_debt_amount: u64,
    pub monthly_deposited_amount: u64,//The monthly properties give the specific value changes for that specific month
    pub monthly_interest_earned_amount: u64,
    pub fees_generated_amount: u64,
    pub monthly_sub_market_fees_collected_amount: u64,
    pub monthly_solvency_insurance_fees_collected_amount: u64,
    pub monthly_liquidation_fees_collected_amount: u64,
    pub monthly_withdrawal_amount: u64,
    pub monthly_borrowed_amount: u64,
    pub monthly_interest_accrued_amount: u64,
    pub monthly_repaid_debt_amount: u64,
    pub monthly_liquidated_amount: u64,
    pub monthly_liquidator_amount: u64,
    pub last_lending_activity_amount: u64,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64 
}