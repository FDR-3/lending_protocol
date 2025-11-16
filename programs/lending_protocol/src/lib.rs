use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, SyncNative, CloseAccount};
use anchor_lang::system_program::{self};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;
use spl_math::precise_number::PreciseNumber;
use pyth_solana_receiver_sdk::price_update::{Price, PriceUpdateV2};
use hex;

declare_id!("4rmvxmwwBFdHsyGsTZ4PRYtasfm3oDiyx3eoibJn48PP");

#[cfg(not(feature = "no-entrypoint"))] //Ensure it's not included when compiled as a library
security_txt!
{
    name: "Lending Protocol",
    project_url: "https://m4a.io",
    contacts: "email fdr3@m4a.io",
    preferred_languages: "en",
    source_code: "https://github.com/FDR-3/lending_protocol",
    policy: "If you find a bug, email me and say something please D:"
}

#[cfg(feature = "dev")] 
const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("Fdqu1muWocA5ms8VmTrUxRxxmSattrmpNraQ7RpPvzZg");

#[cfg(feature = "local")] 
const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("DSLn1ofuSWLbakQWhPUenSBHegwkBBTUwx8ZY4Wfoxm");

const SOL_TOKEN_MINT_ADDRESS: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

//Processed claims need atleast 3 extra bytes of space to pass with full load
const LENDING_USER_ACCOUNT_EXTRA_SIZE: usize = 4;

const MAX_ACCOUNT_NAME_LENGTH: usize = 25;
const PYTH_FEED_ID_LEN: usize = 32;
pub const PRICE_UPDATE_V2_SIZE: usize = size_of::<PriceUpdateV2>();

enum Activity
{
    Deposit = 0,
    Withdraw = 1,
    Borrow = 2,
    Repay = 3,
    Liquidate = 4
}

//Error Codes
#[error_code]
pub enum AuthorizationError 
{
    #[msg("Only the CEO can call this function")]
    NotCEO,
    #[msg("Only the fee Collector can claim the fees")]
    NotFeeCollector
}

#[error_code]
pub enum InvalidInputError
{
    #[msg("The fee on interest earned rate can't be greater than 100%")]
    InvalidFeeRate,
    #[msg("You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose")]
    InsufficientFunds,
    #[msg("You can't pay back more funds than you've borrowed")]
    TooManyFunds,
    #[msg("You must provide all of the sub user's tab accounts")]
    IncorrectNumberOfTabAccounts,
    #[msg("You must provide all of the sub user's tab accounts and Pyth price update accounts")]
    IncorrectNumberOfTabAndPythPriceUpdateAccounts,
    #[msg("You must provide the sub user's tab accounts ordered by user_tab_account_index")]
    IncorrectOrderOfTabAccounts,
    #[msg("Unexpected Tab Account PDA detected. Feed in only legitimate PDA's ordered by user_tab_account_index")]
    UnexpectedTabAccount,
    #[msg("Unexpected Pyth Price Update Account detected. Feed in only legitimate accounts :)")]
    UnexpectedPythPriceUpdateAccount,
    #[msg("Lending User Account name can't be longer than 25 characters")]
    LendingUserAccountNameTooLong,
    #[msg("You can't withdraw or borrow an amount that would expose you to liquidation")]
    LiquidationExposure
}

#[error_code]
pub enum LendingError
{
    #[msg("The price data was stale")]
    StalePriceData,
    #[msg("The Lending User snap shot data was stale")]
    StaleSnapShotData
}

/*//Helper function to get the token price by the pyth ID
fn get_token_pyth_price_by_id<'info>(price_update_account: PriceUpdateV2, pyth_feed_id: [u8; 32]) -> Result<Price>
{
    pub const MAXIMUM_AGE: u64 = 4000; //30 seconds

    let current_price: Price = price_update_account
    .get_price_no_older_than(
        &Clock::get()?, 
        MAXIMUM_AGE, 
        &pyth_feed_id
    )
    .map_err(|_| error!(LendingError::StalePriceData))?; //Handle Option returned by pyth (None if stale or wrong feed)

    Ok(current_price)
}*/

//Helper function to update Token Reserve Accrued Interest Index before a lending transaction (deposit, withdraw, borrow, repay, liqudate)
//This function helps determine how much compounding interest a Token Reserve has earned for its token over the whole life of the Token Reserve's entire existence
fn update_token_reserve_accrued_interest_index<'info>(token_reserve: &mut Account<TokenReserve>, new_lending_activity_time_stamp: u64) -> Result<()>
{
    //Use spl-math library PreciseNumber for fixed point math
    //Set Token Reserve Accured Interest Index = Old Accured Interest Index * (1 + Supply APY * Δt/Seconds in a Year)
    let old_accured_interest_index_precise  = PreciseNumber::new(token_reserve.interest_change_index).unwrap();
    let number_one_precise  = PreciseNumber::new(1 as u128).unwrap();
    let supply_apy_precise = PreciseNumber::new(token_reserve.supply_apy).unwrap();
    let old_time_precise = PreciseNumber::new(token_reserve.last_lending_activity_time_stamp as u128).unwrap();
    let new_time_precise = PreciseNumber::new(new_lending_activity_time_stamp as u128).unwrap();
    let change_in_time_precise = new_time_precise.checked_sub(&old_time_precise).unwrap();
    let seconds_in_a_year_precise = PreciseNumber::new(31_556_952 as u128).unwrap();//1 year = (365.2425 days) × (24 hours/day) × (3600 seconds/hour) = 31,556,952 seconds
    let change_in_time_divided_by_seconds_in_a_year_precise = change_in_time_precise.checked_div(&seconds_in_a_year_precise).unwrap();
    let supply_apy_times_long_ass_variable_name_precise = supply_apy_precise.checked_mul(&change_in_time_divided_by_seconds_in_a_year_precise).unwrap();
    let one_plus_slightly_shorter_long_ass_variable_name_precise = number_one_precise.checked_add(&supply_apy_times_long_ass_variable_name_precise).unwrap();
    let new_accured_interest_index_precise = old_accured_interest_index_precise.checked_mul(&one_plus_slightly_shorter_long_ass_variable_name_precise).unwrap();

    token_reserve.interest_change_index = new_accured_interest_index_precise.to_imprecise().unwrap();

    Ok(())
}

//Helper function to update Token Reserve Utilization Rate and Supply Apy after a lending transaction (deposit, withdraw, borrow, repay, liqudate)
fn update_token_reserve_rates<'info>(token_reserve: &mut Account<TokenReserve>) -> Result<()>
{
    if token_reserve.borrowed_amount == 0
    {
        token_reserve.utilization_rate = 0;
        token_reserve.supply_apy = 0; //There can be no supply apy if no one is borrowing
    }
    else
    {
        //Use spl-math library PreciseNumber for fixed point math
        //Set Token Reserve Utilization Rate = Borrowed Amount / Deposited Amount
        let borrowed_amount_precise = PreciseNumber::new(token_reserve.borrowed_amount).unwrap();
        let deposited_amount_precise = PreciseNumber::new(token_reserve.deposited_amount).unwrap();
        let utilization_rate_precise = borrowed_amount_precise.checked_div(&deposited_amount_precise).unwrap();
        token_reserve.utilization_rate = utilization_rate_precise.to_imprecise().unwrap() as u64;

        //Use spl-math library PreciseNumber for fixed point math
        //Set Token Reserve Supply APY = Borrow APY * Utilization Rate
        let borrow_apy_precise  = PreciseNumber::new(token_reserve.borrow_apy as u128).unwrap();
        let supply_apy_precise = borrow_apy_precise.checked_mul(&utilization_rate_precise).unwrap();
        token_reserve.supply_apy = supply_apy_precise.to_imprecise().unwrap();
    }
    
    msg!("Updated Token Reserve Rates");
    msg!("Utilization Rate: {}", token_reserve.utilization_rate);
    msg!("Supply Apy: {}", token_reserve.supply_apy);

    Ok(())
}

//Helper function to update User Interest Earned amounts. Also updates deposit amounts on the Token Reserve, SubMarket, and user Monthly Statement
fn update_user_previous_interest_earned<'info>(
    token_reserve: &mut Account<TokenReserve>,
    sub_market: &mut Account<SubMarket>,
    lending_user_tab_account: &mut Account<LendingUserTabAccount>,
    lending_user_monthly_statement_account: &mut Account<LendingUserMonthlyStatementAccount>
) -> Result<()>
{
    //Skip if the user has no deposited amount
    if lending_user_tab_account.deposited_amount == 0
    {
        return Ok(())
    }

    //Use spl-math library PreciseNumber for fixed point math
    //User New Balance = Old Balance * (Token Reserve Accrued Interest Index/User Accrued Interest Index)
    let token_reserve_index_precise = PreciseNumber::new(token_reserve.interest_change_index).unwrap();
    let user_index_precise = PreciseNumber::new(lending_user_tab_account.interest_change_index).unwrap();
    let old_token_reserve_deposited_amount_precise = PreciseNumber::new(token_reserve.deposited_amount).unwrap();
    let old_token_reserve_interest_earned_amount_precise = PreciseNumber::new(token_reserve.interest_earned_amount).unwrap();
    let old_token_reserve_fees_generated_amount_precise = PreciseNumber::new(token_reserve.fees_generated_amount).unwrap();
    let old_sub_market_deposited_amount_precise = PreciseNumber::new(sub_market.deposited_amount).unwrap();
    let old_sub_market_interest_earned_amount_precise = PreciseNumber::new(sub_market.interest_earned_amount).unwrap();
    let old_sub_market_fees_generated_amount_precise = PreciseNumber::new(sub_market.fees_generated_amount).unwrap();
    let old_sub_market_uncollected_fees_amount_precise = PreciseNumber::new(sub_market.uncollected_fees_amount).unwrap();
    let old_user_deposited_amount_precise = PreciseNumber::new(lending_user_tab_account.deposited_amount).unwrap();
    let old_user_interest_earned_amount_precise = PreciseNumber::new(lending_user_tab_account.interest_earned_amount).unwrap();
    let old_user_fees_generated_amount_precise = PreciseNumber::new(lending_user_tab_account.fees_generated_amount).unwrap();
    let old_user_monthly_interest_earned_amount_precise = PreciseNumber::new(lending_user_monthly_statement_account.monthly_interest_earned_amount).unwrap();
    let old_user_monthly_fees_generated_amount_precise = PreciseNumber::new(lending_user_monthly_statement_account.monthly_fees_generated_amount).unwrap();

    let token_index_divided_by_user_index_precise = token_reserve_index_precise.checked_div(&user_index_precise).unwrap();
    let new_user_deposited_amount_before_fee_precise = old_user_deposited_amount_precise.checked_mul(&token_index_divided_by_user_index_precise).unwrap();
    
    //Apply SubMarket Fee
    let new_user_interest_earned_amount_before_fee_precise = new_user_deposited_amount_before_fee_precise.checked_sub(&old_user_deposited_amount_precise).unwrap();
    let sub_market_fee_rate_precise = PreciseNumber::new(sub_market.fee_on_interest_earned_rate as u128).unwrap();
    let new_fees_generated_amount_precise = new_user_interest_earned_amount_before_fee_precise.checked_mul(&sub_market_fee_rate_precise).unwrap();
    let new_user_interest_earned_amount_after_fee_precise = new_user_interest_earned_amount_before_fee_precise.checked_sub(&new_fees_generated_amount_precise).unwrap();
    let new_user_deposited_amount_precise = old_user_deposited_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap();
    
    //Convert precise values back into imprecise values
    let new_token_reserve_deposited_amount = old_token_reserve_deposited_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap().to_imprecise().unwrap();
    let new_token_reserve_interest_earned_amount = old_token_reserve_interest_earned_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap().to_imprecise().unwrap();
    let new_token_reserve_fees_generated_amount = old_token_reserve_fees_generated_amount_precise.checked_add(&new_fees_generated_amount_precise).unwrap().to_imprecise().unwrap();
    let new_sub_market_deposited_amount = old_sub_market_deposited_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap().to_imprecise().unwrap();
    let new_sub_market_interest_earned_amount = old_sub_market_interest_earned_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap().to_imprecise().unwrap();
    let new_sub_market_fees_generated_amount = old_sub_market_fees_generated_amount_precise.checked_add(&new_fees_generated_amount_precise).unwrap().to_imprecise().unwrap();
    let new_sub_market_uncollected_fees_amount = old_sub_market_uncollected_fees_amount_precise.checked_add(&new_fees_generated_amount_precise).unwrap().to_imprecise().unwrap();
    let new_user_deposited_amount = new_user_deposited_amount_precise.to_imprecise().unwrap();
    let new_user_total_interest_earned_amount = old_user_interest_earned_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap().to_imprecise().unwrap();
    let new_user_total_fees_generated_amount = old_user_fees_generated_amount_precise.checked_add(&new_fees_generated_amount_precise).unwrap().to_imprecise().unwrap();
    let new_user_montly_interest_earned_amount = old_user_monthly_interest_earned_amount_precise.checked_add(&new_user_interest_earned_amount_after_fee_precise).unwrap().to_imprecise().unwrap();
    let new_user_montly_fees_generated_amount = old_user_monthly_fees_generated_amount_precise.checked_add(&new_fees_generated_amount_precise).unwrap().to_imprecise().unwrap();

    token_reserve.deposited_amount = new_token_reserve_deposited_amount;
    token_reserve.interest_earned_amount = new_token_reserve_interest_earned_amount;
    token_reserve.fees_generated_amount = new_token_reserve_fees_generated_amount;
    sub_market.deposited_amount = new_sub_market_deposited_amount;
    sub_market.interest_earned_amount = new_sub_market_interest_earned_amount;
    sub_market.fees_generated_amount = new_sub_market_fees_generated_amount;
    sub_market.uncollected_fees_amount = new_sub_market_uncollected_fees_amount;
    lending_user_tab_account.deposited_amount = new_user_deposited_amount;
    lending_user_tab_account.interest_earned_amount = new_user_total_interest_earned_amount;
    lending_user_tab_account.fees_generated_amount = new_user_total_fees_generated_amount;
    lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
    lending_user_monthly_statement_account.snap_shot_interest_earned_amount = lending_user_tab_account.interest_earned_amount;
    lending_user_monthly_statement_account.snap_shot_fees_generated_amount = lending_user_tab_account.fees_generated_amount;
    lending_user_monthly_statement_account.monthly_interest_earned_amount = new_user_montly_interest_earned_amount;
    lending_user_monthly_statement_account.monthly_fees_generated_amount = new_user_montly_fees_generated_amount;

    Ok(())
}

//Helper function to update User Accured Debt amounts. Also updates debt amounts on the Token Reserve, SubMarket, and user Monthly Statement
fn update_user_previous_interest_accrued<'info>(
    token_reserve: &mut Account<TokenReserve>,
    sub_market: &mut Account<SubMarket>,
    lending_user_tab_account: &mut Account<LendingUserTabAccount>,
    lending_user_monthly_statement_account: &mut Account<LendingUserMonthlyStatementAccount>
) -> Result<()>
{
    //Skip if the user has no borrowed amount
    if lending_user_tab_account.borrowed_amount == 0
    {
        return Ok(())
    }

    //Use spl-math library PreciseNumber for fixed point math
    //User New Debt = Old Debt * (Token Reserve Accrued Interest Index/User Accrued Interest Index)
    let token_reserve_index_precise = PreciseNumber::new(token_reserve.interest_change_index).unwrap();
    let user_index_precise = PreciseNumber::new(lending_user_tab_account.interest_change_index).unwrap();
    let old_token_reserve_borrowed_amount_precise = PreciseNumber::new(token_reserve.borrowed_amount).unwrap();
    let old_token_reserve_interest_accrued_amount_precise = PreciseNumber::new(token_reserve.interest_accrued_amount).unwrap();
    let old_sub_market_borrowed_amount_precise = PreciseNumber::new(sub_market.borrowed_amount).unwrap();
    let old_sub_market_interest_accrued_amount_precise = PreciseNumber::new(sub_market.interest_accrued_amount).unwrap();
    let old_user_borrowed_amount_precise = PreciseNumber::new(lending_user_tab_account.borrowed_amount).unwrap();
    let old_user_interest_accrued_amount_precise = PreciseNumber::new(lending_user_tab_account.interest_accrued_amount).unwrap();
    let old_user_monthly_interest_accrued_amount_precise = PreciseNumber::new(lending_user_monthly_statement_account.monthly_interest_accrued_amount).unwrap();

    let token_index_divided_by_user_index_precise = token_reserve_index_precise.checked_div(&user_index_precise).unwrap();
    let new_user_borrowed_amount_precise = old_user_borrowed_amount_precise.checked_mul(&token_index_divided_by_user_index_precise).unwrap();
    let new_user_interest_accrued_amount_precise = new_user_borrowed_amount_precise.checked_sub(&old_user_borrowed_amount_precise).unwrap();

    let new_token_reserve_borrowed_amount = old_token_reserve_borrowed_amount_precise.checked_add(&new_user_interest_accrued_amount_precise).unwrap().to_imprecise().unwrap();
    let new_token_reserve_interest_accrued_amount = old_token_reserve_interest_accrued_amount_precise.checked_add(&new_user_interest_accrued_amount_precise).unwrap().to_imprecise().unwrap();
    let new_sub_market_borrowed_amount = old_sub_market_borrowed_amount_precise.checked_add(&new_user_interest_accrued_amount_precise).unwrap().to_imprecise().unwrap();
    let new_sub_market_interest_accrued_amount = old_sub_market_interest_accrued_amount_precise.checked_add(&new_user_interest_accrued_amount_precise).unwrap().to_imprecise().unwrap();
    let new_user_borrowed_amount = new_user_borrowed_amount_precise.to_imprecise().unwrap();
    let new_user_total_interest_accrued_amount = old_user_interest_accrued_amount_precise.checked_add(&new_user_interest_accrued_amount_precise).unwrap().to_imprecise().unwrap();
    let new_user_montly_interest_accrued_amount = old_user_monthly_interest_accrued_amount_precise.checked_add(&new_user_interest_accrued_amount_precise).unwrap().to_imprecise().unwrap();

    token_reserve.borrowed_amount = new_token_reserve_borrowed_amount;
    token_reserve.interest_accrued_amount = new_token_reserve_interest_accrued_amount;
    sub_market.borrowed_amount = new_sub_market_borrowed_amount;
    sub_market.interest_accrued_amount = new_sub_market_interest_accrued_amount;
    lending_user_tab_account.borrowed_amount = new_user_borrowed_amount;
    lending_user_tab_account.interest_accrued_amount = new_user_total_interest_accrued_amount;
    lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
    lending_user_monthly_statement_account.snap_shot_interest_accrued_amount = lending_user_tab_account.interest_accrued_amount;
    lending_user_monthly_statement_account.monthly_interest_accrued_amount = new_user_montly_interest_accrued_amount;

    Ok(())
}

//Helper function to validate Tab Accounts and Pyth Price Update Accounts and to see if the Withdraw or Borrow request will expose the user to liquidation
fn validate_tab_and_price_update_accounts_and_check_liquidation_exposure<'a, 'info>(remaining_accounts_iter: &mut core::slice::Iter<'a, AccountInfo<'info>>,
    signer: Pubkey,
    user_account_index: u8,
    program_id: Pubkey,
    token_mint_address: Pubkey,
    withdraw_or_borrow_amount: u64,
    activity_type: u8,
    new_lending_activity_time_stamp: u64
) -> Result<()>
{
    let mut user_tab_index = 0;
    let mut user_deposited_value = PreciseNumber::new(0 as u128).unwrap();
    let mut user_borrowed_value = PreciseNumber::new(0 as u128).unwrap();
    let mut user_withdraw_or_borrow_request_value = PreciseNumber::new(0 as u128).unwrap();
    let time_stamp = Clock::get()?.unix_timestamp;
    const MAXIMUM_PRICE_AGE: u64 = 30; //30 seconds

    while let Some(tab_account_serialized) = remaining_accounts_iter.next()
    {
        let data_ref = tab_account_serialized.data.borrow();
        let mut data_slice: &[u8] = data_ref.deref();

        let tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

        let (expected_pda, _bump) = Pubkey::find_program_address(
            &[b"lendingUserTabAccount",
            tab_account.token_mint_address.key().as_ref(),
            tab_account.sub_market_owner_address.key().as_ref(),
            tab_account.sub_market_index.to_le_bytes().as_ref(),
            signer.key().as_ref(),//The syntax 2 lines down is interchangeable with this line for Public Keys
            user_account_index.to_le_bytes().as_ref()],
            &program_id//The syntax 2 lines up is interchangeable with this line for Public Keys
        );

        //You must provide all of the sub user's tab accounts ordered by user_tab_account_index
        require!(user_tab_index == tab_account.user_tab_account_index, InvalidInputError::IncorrectOrderOfTabAccounts);
        require_keys_eq!(expected_pda.key(), tab_account_serialized.key(), InvalidInputError::UnexpectedTabAccount);

        //The lending user tab account interest earned and debt accured data (Plus Token Reserve data) must be no older than 120 seconds. The user has to run the update_user_snap_shots function if data is stale.
        //2 minutes gives the user plenty of time to call both functions. Users shouldn't earn or accrue that much interest or debt within 2 minutes and if they do, that's what the liquidation function is for if there's an issue later :0
        let time_diff = new_lending_activity_time_stamp - tab_account.interest_change_last_updated_time_stamp;
        require!(time_diff <= 120, LendingError::StaleSnapShotData);
        
        //Validate Price Update Account
        let price_update_account_serialized = remaining_accounts_iter.next().unwrap(); //The Price Update Account comes after the Tab Account
        require_keys_eq!(tab_account.pyth_feed_address.key(), price_update_account_serialized.key(), InvalidInputError::UnexpectedPythPriceUpdateAccount);

        let data_ref = price_update_account_serialized.data.borrow();
        let mut data_slice: &[u8] = data_ref.deref();

        let price_update_account = PriceUpdateV2::try_deserialize(&mut data_slice)?;
        
        //The published time for the Pyth Price Update Account can be no older than 30 seconds
        let time_diff = (time_stamp - price_update_account.price_message.publish_time) as u64;
        require!(time_diff <= MAXIMUM_PRICE_AGE, LendingError::StalePriceData);

        let current_price = price_update_account.price_message;

        msg!
        (
            "Token: {}",
            tab_account.token_mint_address.key()
        );

        msg!
        (
            "Token Price: {} +- {} x 10^{}",
            current_price.price,
            current_price.conf,
            current_price.exponent
        );

        let token_price_value_precise = PreciseNumber::new(current_price.price as u128).unwrap();
        let tab_deposited_amount_precise = PreciseNumber::new(tab_account.deposited_amount as u128).unwrap(); 
        let tab_borrowed_amount_precise = PreciseNumber::new(tab_account.borrowed_amount as u128).unwrap();
        let tab_deposited_value_precise = tab_deposited_amount_precise.checked_mul(&token_price_value_precise).unwrap();
        let tab_borrowed_value_precise = tab_borrowed_amount_precise.checked_mul(&token_price_value_precise).unwrap();

        user_deposited_value = user_deposited_value.checked_add(&tab_deposited_value_precise).unwrap();
        user_borrowed_value = user_borrowed_value.checked_add(&tab_borrowed_value_precise).unwrap();

        if token_mint_address.key() == tab_account.token_mint_address.key()
        {
            let withdraw_or_borrow_amount_precise = PreciseNumber::new(withdraw_or_borrow_amount as u128).unwrap();
            let withdraw_or_borrow_request_value_precise = withdraw_or_borrow_amount_precise.checked_mul(&token_price_value_precise).unwrap();
            user_withdraw_or_borrow_request_value = user_withdraw_or_borrow_request_value.checked_add(&withdraw_or_borrow_request_value_precise).unwrap();
        }

        user_tab_index += 1;
    }

    msg!
    (
        "Value calculation test. Deposited: {}, Borrowed: {}, Requested: {}",
        user_deposited_value.to_imprecise().unwrap(),
        user_borrowed_value.to_imprecise().unwrap(),
        user_withdraw_or_borrow_request_value.to_imprecise().unwrap()
    );

    if activity_type == Activity::Withdraw as u8
    {
        user_deposited_value = user_deposited_value.checked_sub(&user_withdraw_or_borrow_request_value).unwrap();
    }
    else
    {
        user_borrowed_value = user_borrowed_value.checked_add(&user_withdraw_or_borrow_request_value).unwrap();
    }

    if user_borrowed_value.to_imprecise().unwrap() > 0
    {
        let seventy_percent_precise = PreciseNumber::new(700_000_000_000_000_000 as u128).unwrap();
        let seventy_percent_of_new_deposited_value = user_deposited_value.checked_mul(&seventy_percent_precise).unwrap();

        //You can't withdraw or borrow an amount that would expose you to liquidation. Liabilities can't exceed 70% of collateral.
        require!(seventy_percent_of_new_deposited_value.to_imprecise().unwrap() >= user_borrowed_value.to_imprecise().unwrap(), InvalidInputError::LiquidationExposure);
    }

    Ok(())
}

//Helper function to initialize Monthly Statement Accounts
fn initialize_lending_user_monthly_statement_account<'info>(lending_user_monthly_statement_account: &mut Account<LendingUserMonthlyStatementAccount>,
    lending_protocol: &Account<LendingProtocol>,
    signer: Pubkey,
    user_account_index: u8,
    token_mint_address: Pubkey
) -> Result<()>
{
    lending_user_monthly_statement_account.owner = signer.key();
    lending_user_monthly_statement_account.user_account_index = user_account_index;
    lending_user_monthly_statement_account.token_mint_address = token_mint_address;
    lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
    lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
    lending_user_monthly_statement_account.monthly_statement_account_added = true;

    msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);

    Ok(())
}

#[program]
pub mod lending_protocol 
{
    use super::*;

    pub fn initialize_lending_protocol(ctx: Context<InitializeLendingProtocol>, statement_month: u8, statement_year: u32) -> Result<()> 
    {msg!("PriceUpdateV2 Expected Size: {}", size_of::<PriceUpdateV2>());
        //Only the initial CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), INITIAL_CEO_ADDRESS, AuthorizationError::NotCEO);

        let ceo = &mut ctx.accounts.ceo;
        ceo.address = INITIAL_CEO_ADDRESS;

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.current_statement_month = statement_month;
        lending_protocol.current_statement_year = statement_year;

        msg!("Lending Protocol Initialized");
        msg!("New CEO Address: {}", ceo.address.key());
        msg!("Current Statement Month: {}, Year: {}", lending_protocol.current_statement_month, lending_protocol.current_statement_year);

        Ok(())
    }

    pub fn pass_on_lending_protocol_ceo(ctx: Context<PassOnLendingProtocolCEO>, new_ceo_address: Pubkey) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        msg!("The Lending Protocol CEO has passed on the title to a new CEO");
        msg!("New CEO: {}", new_ceo_address.key());

        ceo.address = new_ceo_address.key();

        Ok(())
    }

    pub fn update_current_statement_month_and_year(ctx: Context<UpdateCurrentStatementMonthAndYear>, statement_month: u8, statement_year: u32) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.current_statement_month = statement_month;
        lending_protocol.current_statement_year = statement_year;

        msg!("Updated Lending Protocol To Statement Month: {}, Year: {}", lending_protocol.current_statement_month, lending_protocol.current_statement_year);

        Ok(())
    }

    pub fn add_token_reserve(ctx: Context<AddTokenReserve>,
        token_mint_address: Pubkey,
        token_decimal_amount: u8,
        pyth_feed_id: [u8; PYTH_FEED_ID_LEN],
        pyth_feed_address: Pubkey,
        borrow_apy: u16,
        global_limit: u128) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.token_mint_address = token_mint_address.key();
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.pyth_feed_id = pyth_feed_id;
        token_reserve.pyth_feed_address = pyth_feed_address.key();
        token_reserve.borrow_apy = borrow_apy;
        token_reserve.global_limit = global_limit;

        token_reserve.token_reserve_protocol_index = token_reserve_stats.token_reserve_count;
        token_reserve_stats.token_reserve_count += 1;

        let hex_string = hex::encode(pyth_feed_id);

        msg!("Added Token Reserve #{}", token_reserve_stats.token_reserve_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
        msg!("Pyth Feed ID: 0x{}", hex_string);
        msg!("Borrow APY: {}", borrow_apy);
        msg!("Global Limit: {}", global_limit);
            
        Ok(())
    }

    pub fn update_token_reserve(ctx: Context<UpdateTokenReserve>,
        _token_mint_address: Pubkey,
        borrow_apy: u16,
        global_limit: u128) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;

        token_reserve.borrow_apy = borrow_apy;
        token_reserve.global_limit = global_limit;
        token_reserve_stats.token_reserves_updated_count += 1;

        msg!("Token Reserve Updated");
        msg!("New Borrow APY: {}",  borrow_apy);
        msg!("New Global Limit: {}",  global_limit);
            
        Ok(())
    }

    pub fn create_sub_market(ctx: Context<CreateSubMarket>,
        token_mint_address: Pubkey,
        sub_market_index: u16,
        fee_collector_address: Pubkey,
        fee_on_interest_earned_rate: u16
    ) -> Result<()> 
    {
        //Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(fee_on_interest_earned_rate <= 10_000, InvalidInputError::InvalidFeeRate);

        let sub_market = &mut ctx.accounts.sub_market;
        sub_market.owner = ctx.accounts.signer.key();
        sub_market.fee_collector_address = fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate; //This should fed in as a decimal from 0.0000 to 1.0000
        sub_market.token_mint_address = token_mint_address.key(); //This can't be edited after. Allowing this to be edited would be like allowing some one to say this currency is a different kind of currency later when ever they wanted
        sub_market.sub_market_index = sub_market_index;
        
        let sub_market_stats = &mut ctx.accounts.sub_market_stats;
        sub_market_stats.sub_market_creation_count += 1;
        sub_market.id = sub_market_stats.sub_market_creation_count;

        msg!("Created SubMarket #{}", sub_market.id);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("SubMarket Index: {}", sub_market.sub_market_index);
        msg!("Owner: {}", ctx.accounts.signer.key());
        msg!("Fee Collector Address: {}", fee_collector_address.key());
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate/100); //convert out of % fixed point notation with 4 decimal places back to decimal for logging
        
        Ok(())
    }

    pub fn edit_sub_market(ctx: Context<EditSubMarket>,
        _token_mint_address: Pubkey,
        sub_market_index: u16,
        fee_collector_address: Pubkey,
        fee_on_interest_earned_rate: u16
    ) -> Result<()> 
    {
        //Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(fee_on_interest_earned_rate <= 10_000, InvalidInputError::InvalidFeeRate);

        let sub_market = &mut ctx.accounts.sub_market;
        sub_market.fee_collector_address = fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate;

        let sub_market_stats = &mut ctx.accounts.sub_market_stats;
        sub_market_stats.sub_market_edit_count += 1;
        
        msg!("Edited Submarket");
        msg!("Token Mint Address: {}", sub_market.token_mint_address.key());
        msg!("SubMarket Index: {}", sub_market_index);
        msg!("Owner: {}", ctx.accounts.signer.key());
        msg!("Fee Collector Address: {}", fee_collector_address.key());
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate/100); //convert out of fixed point notation with 4 decimal places back to percent for logging. So / 10^4 for decimal then * 10^2 for percent
            
        Ok(())
    }

    pub fn deposit_tokens(ctx: Context<DepositTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        account_name: Option<String> //Optional variable. Use null/undefined on front end when not needed
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let user_lending_account = &mut ctx.accounts.user_lending_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Populate lending user account if being newly initliazed. A user can have multiple accounts based on their account index. 
        if user_lending_account.lending_user_account_added == false
        {
            user_lending_account.owner = ctx.accounts.signer.key();
            user_lending_account.user_account_index = user_account_index;

            if let Some(new_account_name) = account_name
            {
                //Account Name string must not be longer than 25 characters
                require!(new_account_name.len() <= MAX_ACCOUNT_NAME_LENGTH, InvalidInputError::LendingUserAccountNameTooLong);

                user_lending_account.account_name = new_account_name.clone();

                msg!("Created Lending User Account Named: {}", new_account_name);
            }

            user_lending_account.lending_user_account_added = true;
        }
        
        //Populate tab account if being newly initliazed. Every token the lending user enteracts with has its own tab account tied to that sub user based on account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            lending_user_tab_account.owner = ctx.accounts.signer.key();
            lending_user_tab_account.user_account_index = user_account_index;
            lending_user_tab_account.token_mint_address = token_mint_address;
            lending_user_tab_account.pyth_feed_id = token_reserve.pyth_feed_id;
            lending_user_tab_account.pyth_feed_address = token_reserve.pyth_feed_address;
            lending_user_tab_account.sub_market_owner_address = sub_market_owner_address.key();
            lending_user_tab_account.sub_market_index = sub_market_index;
            lending_user_tab_account.user_tab_account_index = user_lending_account.tab_account_count;
            lending_user_tab_account.user_tab_account_added = true;

            user_lending_account.tab_account_count += 1;

            msg!("Created Lending User Tab Account Indexed At: {}", lending_user_tab_account.user_tab_account_index);
        }

        //Initialize monthly statement account if the statement month/year has changed or brand new sub user account.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                ctx.accounts.signer.key(),
                user_account_index,
                token_mint_address.key()
            )?;
        }

        //Calculate Token Reserve Previously Earned Interest
        update_token_reserve_accrued_interest_index(token_reserve, time_stamp)?;

        update_user_previous_interest_earned(
            token_reserve,
            sub_market,
            lending_user_tab_account,
            lending_user_monthly_statement_account
        )?;

        update_user_previous_interest_accrued(
            token_reserve,
            sub_market,
            lending_user_tab_account,
            lending_user_monthly_statement_account
        )?;

        //Handle native SOL transactions
        if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
        {
            //CPI to the System Program to transfer SOL from the user to the program's wSOL ATA.
            let cpi_accounts = system_program::Transfer
            {
                from: ctx.accounts.signer.to_account_info(),
                to: ctx.accounts.token_reserve_ata.to_account_info()
            };
            let cpi_program = ctx.accounts.system_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            system_program::transfer(cpi_ctx, amount)?;

            //CPI to the SPL Token Program to "sync" the wSOL ATA's balance.
            let cpi_accounts = SyncNative
            {
                account: ctx.accounts.token_reserve_ata.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::sync_native(cpi_ctx)?;

            //Close temporary wSOL ATA if its balance is zero
            let user_balance_after_transfer = ctx.accounts.user_ata.amount;
            if user_balance_after_transfer == 0
            {
                
                let cpi_accounts = CloseAccount
                {
                    account: ctx.accounts.user_ata.to_account_info(),
                    destination: ctx.accounts.signer.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                };
                let cpi_program = ctx.accounts.token_program.to_account_info();
                let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
                token::close_account(cpi_ctx)?; 
            }
        }
        //Handle all other tokens
        else
        {
            //Cross Program Invocation for Token Transfer
            let cpi_accounts = Transfer
            {
                from: ctx.accounts.user_ata.to_account_info(),
                to: ctx.accounts.token_reserve_ata.to_account_info(),
                authority: ctx.accounts.signer.to_account_info()
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

            //Transfer Tokens Into The Reserve
            token::transfer(cpi_ctx, amount)?;  
        }

        //Update Values and Stat Listener
        lending_stats.deposits += 1;
        sub_market.deposited_amount += amount as u128;
        token_reserve.deposited_amount += amount as u128;
        lending_user_tab_account.deposited_amount += amount as u128;
        lending_user_monthly_statement_account.monthly_deposited_amount += amount as u128;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest index
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.interest_change_index = token_reserve.interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount as u128;
        token_reserve.last_lending_activity_type = Activity::Deposit as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = amount as u128;
        sub_market.last_lending_activity_type = Activity::Deposit as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp; //This gets set when calling deposit_tokens, repay_tokens, update_user_snap_shots, or claim_sub_market_fees
        lending_user_monthly_statement_account.last_lending_activity_amount = amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Deposit as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;

        msg!("{} deposited for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);   

        Ok(())
    }

    pub fn edit_lending_user_account_name(ctx: Context<EditLendingUserAccountName>,
        _user_account_index: u8,
        account_name: String
    ) -> Result<()> 
    {
        //Account Name string must not be longer than 25 characters
        require!(account_name.len() <= MAX_ACCOUNT_NAME_LENGTH, InvalidInputError::LendingUserAccountNameTooLong);

        let user_lending_account = &mut ctx.accounts.user_lending_account;
        user_lending_account.account_name = account_name.clone();

        let lending_user_stats = &mut ctx.accounts.lending_user_stats;
        lending_user_stats.name_change_count += 1;

        msg!("Lending User Account name updated to: {}", account_name);

        Ok(()) 
    }

    pub fn withdraw_tokens(ctx: Context<WithdrawTokens>,
        token_mint_address: Pubkey,
        _sub_market_owner_address: Pubkey,
        _sub_market_index: u16,
        user_account_index: u8,
        amount: u64
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        //You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose
        require!(lending_user_tab_account.deposited_amount >= amount as u128, InvalidInputError::InsufficientFunds);

        let user_lending_account = &mut ctx.accounts.user_lending_account;
        //You must provide all of the sub user's tab accounts in remaining accounts. Every Tab Account has a corresponding Pyth Price Update Account directly after it in the passed in array
        require!((user_lending_account.tab_account_count * 2) as usize == ctx.remaining_accounts.len() as usize, InvalidInputError::IncorrectNumberOfTabAndPythPriceUpdateAccounts);

        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                ctx.accounts.signer.key(),
                user_account_index,
                token_mint_address.key()
            )?;
        }

        //Validate Passed in User Tab Accounts and Pyth Price Update Accounts and Check Liquidation Exposure
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        validate_tab_and_price_update_accounts_and_check_liquidation_exposure(
            &mut remaining_accounts_iter,
            ctx.accounts.signer.key(),
            user_account_index,
            ctx.program_id.key(),
            token_mint_address,
            amount,
            Activity::Withdraw as u8,
            time_stamp
        )?;

        //Transfer Tokens Back To User ATA
        let token_mint_key = token_mint_address.key();
        let (_expected_pda, bump) = Pubkey::find_program_address
        (
            &[b"tokenReserve",
            token_mint_address.key().as_ref()],
            &ctx.program_id,
        );

        let seeds = &[b"tokenReserve", token_mint_key.as_ref(), &[bump]];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer
        {
            from: ctx.accounts.token_reserve_ata.to_account_info(),
            to: ctx.accounts.user_ata.to_account_info(),
            authority: token_reserve.to_account_info()
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

        //Transfer Tokens Back to the User
        token::transfer(cpi_ctx, amount)?;

        //Handle wSOL Token unwrap
        if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
        {
            let user_balance_after_transfer = ctx.accounts.user_ata.amount;

            if user_balance_after_transfer > amount
            {
                //Since User already had wrapped SOL, only unwrapped the amount withdrawn
                let cpi_accounts = system_program::Transfer
                {
                    from: ctx.accounts.user_ata.to_account_info(),
                    to: ctx.accounts.signer.to_account_info()
                };
                let cpi_program = ctx.accounts.system_program.to_account_info();
                let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
                system_program::transfer(cpi_ctx, amount)?;
            }
            else
            {
                //Since the User has no other wSOL, unwrap it all, send it to the User, and close the temporary wSOL account
                let cpi_accounts = CloseAccount
                {
                    account: ctx.accounts.user_ata.to_account_info(),
                    destination: ctx.accounts.signer.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                };
                let cpi_program = ctx.accounts.token_program.to_account_info();
                let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
                token::close_account(cpi_ctx)?; 
            }
        }
        
        //Update Values and Stat Listener
        lending_stats.withdrawals += 1;
        sub_market.deposited_amount -= amount as u128;
        token_reserve.deposited_amount -= amount as u128;
        lending_user_tab_account.deposited_amount -= amount as u128;
        lending_user_monthly_statement_account.monthly_withdrawal_amount += amount as u128;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
        
        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest index
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.interest_change_index = token_reserve.interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount as u128;
        token_reserve.last_lending_activity_type = Activity::Withdraw as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = amount as u128;
        sub_market.last_lending_activity_type = Activity::Withdraw as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp; 
        lending_user_monthly_statement_account.last_lending_activity_amount = amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Withdraw as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;
        
        msg!("{} withdrew for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    pub fn borrow_tokens(ctx: Context<BorrowTokens>,
        token_mint_address: Pubkey,
        _sub_market_owner_address: Pubkey,
        _sub_market_index: u16,
        user_account_index: u8,
        amount: u64
    ) -> Result<()> 
    {
        let user_lending_account = &mut ctx.accounts.user_lending_account;
        //You must provide all of the sub user's tab accounts in remaining accounts. Every Tab Account should have a corresponding Pyth Price Update Account directly after it in the passed in array
        require!(user_lending_account.tab_account_count as usize == ctx.remaining_accounts.len() * 2 as usize, InvalidInputError::IncorrectNumberOfTabAndPythPriceUpdateAccounts);

        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        //You can't borrow an amount that would cause your borrows to be greater than %70 of the value of your deposits
        //require!(lending_user_tab_account.deposited_amount >= amount as u128, InvalidInputError::InsufficientFunds);

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                ctx.accounts.signer.key(),
                user_account_index,
                token_mint_address.key()
            )?;
        }

        //Validate Passed in User Tab Accounts and Pyth Price Update Accounts and Check Liquidation Exposure
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        validate_tab_and_price_update_accounts_and_check_liquidation_exposure(
            &mut remaining_accounts_iter,
            ctx.accounts.signer.key(),
            user_account_index,
            ctx.program_id.key(),
            token_mint_address,
            amount,
            Activity::Borrow as u8,
            time_stamp
        )?;

        //Transfer Tokens Back To User ATA
        let token_mint_key = token_mint_address.key();
        let (_expected_pda, bump) = Pubkey::find_program_address
        (
            &[b"tokenReserve",
            token_mint_address.key().as_ref()],
            &ctx.program_id,
        );

        let seeds = &[b"tokenReserve", token_mint_key.as_ref(), &[bump]];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer
        {
            from: ctx.accounts.token_reserve_ata.to_account_info(),
            to: ctx.accounts.user_ata.to_account_info(),
            authority: ctx.accounts.token_reserve.to_account_info()
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

        //Transfer Tokens Back to the User
        token::transfer(cpi_ctx, amount)?;

        //Handle wSOL Token unwrap
        if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
        {
            let user_balance_after_transfer = ctx.accounts.user_ata.amount;

            if user_balance_after_transfer > amount
            {
                //Since User already had wrapped SOL, only unwrapped the amount withdrawn
                let cpi_accounts = system_program::Transfer
                {
                    from: ctx.accounts.user_ata.to_account_info(),
                    to: ctx.accounts.signer.to_account_info()
                };
                let cpi_program = ctx.accounts.system_program.to_account_info();
                let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
                system_program::transfer(cpi_ctx, amount)?;
            }
            else
            {
                //Since the User has no other wrapped SOL, unwrap it all, send it to the User, and close the temporary wrapped SOL account
                let cpi_accounts = CloseAccount
                {
                    account: ctx.accounts.user_ata.to_account_info(),
                    destination: ctx.accounts.signer.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                };
                let cpi_program = ctx.accounts.token_program.to_account_info();
                let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
                token::close_account(cpi_ctx)?; 
            }
        }
        
        let token_reserve = &mut ctx.accounts.token_reserve;

        //Update Values and Stat Listener
        lending_stats.borrows += 1;
        sub_market.borrowed_amount += amount as u128;
        token_reserve.borrowed_amount += amount as u128;
        lending_user_tab_account.borrowed_amount += amount as u128;
        lending_user_monthly_statement_account.monthly_borrowed_amount += amount as u128;
        lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest index
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.interest_change_index = token_reserve.interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount as u128;
        token_reserve.last_lending_activity_type = Activity::Borrow as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = amount as u128;
        sub_market.last_lending_activity_type = Activity::Borrow as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp; 
        lending_user_monthly_statement_account.last_lending_activity_amount = amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Borrow as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;
        
        msg!("{} borrowed for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    pub fn repay_tokens(ctx: Context<RepayTokens>,
        token_mint_address: Pubkey,
        _sub_market_owner_address: Pubkey,
        _sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        pay_off_loan: bool
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                ctx.accounts.signer.key(),
                user_account_index,
                token_mint_address.key()
            )?;
        }

        //Calculate Token Reserve Previously Earned Interest
        update_token_reserve_accrued_interest_index(token_reserve, time_stamp)?;

        update_user_previous_interest_earned(
            token_reserve,
            sub_market,
            lending_user_tab_account,
            lending_user_monthly_statement_account
        )?;

        update_user_previous_interest_accrued(
            token_reserve,
            sub_market,
            lending_user_tab_account,
            lending_user_monthly_statement_account
        )?;

        let payment_amount;

        if pay_off_loan
        {
            payment_amount = lending_user_tab_account.borrowed_amount as u64;
        }
        else
        {
            payment_amount = amount
        }

        //You can't repay an amount that is greater than your borrowed amount.
        require!(lending_user_tab_account.borrowed_amount >= payment_amount as u128, InvalidInputError::TooManyFunds);

        //Cross Program Invocation for Token Transfer
        let cpi_accounts = Transfer
        {
            from: ctx.accounts.user_ata.to_account_info(),
            to: ctx.accounts.token_reserve_ata.to_account_info(),
            authority: ctx.accounts.signer.to_account_info()
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        //Transfer Tokens Into The Reserve
        token::transfer(cpi_ctx, payment_amount)?;

        //Update Values and Stat Listener
        lending_stats.repayments += 1;
        sub_market.borrowed_amount -= payment_amount as u128;
        sub_market.repaid_debt_amount += payment_amount as u128;
        token_reserve.borrowed_amount -= payment_amount as u128;
        token_reserve.repaid_debt_amount += payment_amount as u128;
        lending_user_tab_account.borrowed_amount -= payment_amount as u128;
        lending_user_tab_account.repaid_debt_amount += payment_amount as u128;
        lending_user_monthly_statement_account.monthly_repaid_debt_amount += payment_amount as u128;
        lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
        lending_user_monthly_statement_account.snap_shot_repaid_debt_amount = lending_user_tab_account.repaid_debt_amount;
        
        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest index
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.interest_change_index = token_reserve.interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = payment_amount as u128;
        token_reserve.last_lending_activity_type = Activity::Repay as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = payment_amount as u128;
        sub_market.last_lending_activity_type = Activity::Repay as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp; //This gets set when calling deposit_tokens, repay_tokens, update_user_snap_shots, or claim_sub_market_fees
        lending_user_monthly_statement_account.last_lending_activity_amount = payment_amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;
  
        msg!("{} repaid debt for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    //Updates interest earned and debt accrued amounts for all of a User's Tab Accounts, and the Token Reserves, Sub Markets, and Monthly Statement Accounts associated with them
    pub fn update_user_snap_shots<'info>(ctx: Context<'_, '_, 'info, 'info, UpdateUserSnapShots<'info>>, user_account_index: u8, user_touched_dynamic_market_array: Vec<u8>) -> Result<()>
    {
        let lending_protocol = &mut ctx.accounts.lending_protocol;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let user_lending_account = &mut ctx.accounts.user_lending_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        let mut user_tab_index = 0;

        for user_touched_token_reserve_market_count in &user_touched_dynamic_market_array
        {
            //Token Reserve
            let token_reserve_account_serialized = remaining_accounts_iter.next().unwrap(); //The Price Update Account comes after the Tab Account
            let mut token_reserve = Account::<TokenReserve>::try_from(&token_reserve_account_serialized)?;
            let (expected_token_reserve_pda, _token_reserve_bump) = Pubkey::find_program_address(
                &[b"tokenReserve",
                token_reserve.token_mint_address.as_ref()],
                &ctx.program_id
            );

            //Validate Token Reserve Account
            require_keys_eq!(expected_token_reserve_pda.key(), token_reserve_account_serialized.key(), InvalidInputError::UnexpectedTabAccount);

            //Calculate Token Reserve Previously Earned Interest
            update_token_reserve_accrued_interest_index(&mut token_reserve, time_stamp)?;

            for _i in 0..(*user_touched_token_reserve_market_count).into()
            {
                //Sub Market Account
                //The Sub Market Account comes after the Token Reserve Account. Repeats if additional markets after Monthly Statement Account.
                let sub_market_account_serialized = remaining_accounts_iter.next().unwrap();
                let mut sub_market = Account::<SubMarket>::try_from(&sub_market_account_serialized)?;
                let (expected_sub_market_pda, _sub_market_bump) = Pubkey::find_program_address(
                    &[b"subMarket",
                    sub_market.token_mint_address.as_ref(),
                    sub_market.owner.as_ref(),
                    &sub_market.sub_market_index.to_le_bytes()],
                    &ctx.program_id
                );

                //Lending User Tab Account
                //The Lending User Tab Account comes after the Sub Market Account.
                let lending_user_tab_account_serialized = remaining_accounts_iter.next().unwrap();
                let mut lending_user_tab_account = Account::<LendingUserTabAccount>::try_from(&lending_user_tab_account_serialized)?;
                let (expected_lending_user_tab_account_pda, _bump) = Pubkey::find_program_address(
                    &[b"lendingUserTabAccount",
                    lending_user_tab_account.token_mint_address.as_ref(),
                    lending_user_tab_account.sub_market_owner_address.as_ref(),
                    &lending_user_tab_account.sub_market_index.to_le_bytes(),
                    ctx.accounts.signer.key().as_ref(),
                    &user_account_index.to_le_bytes().as_ref()],
                    &ctx.program_id
                );

                //Lending User Monthly Statement Account
                //The Lending User Monthly Statement Account comes after the Lending User Tab Account.
                let lending_user_monthly_statement_account_serialized = remaining_accounts_iter.next().unwrap();
                let mut lending_user_monthly_statement_account = Account::<LendingUserMonthlyStatementAccount>::try_from(&lending_user_monthly_statement_account_serialized)?;
                let (expected_user_monthly_statement_account_pda, user_monthly_statement_account_bump) = Pubkey::find_program_address(
                    &[b"userMonthlyStatementAccount",
                    lending_protocol.current_statement_month.to_le_bytes().as_ref(),
                    lending_protocol.current_statement_year.to_le_bytes().as_ref(),
                    token_reserve.token_mint_address.as_ref(), //Using Token Mint Address on Token Reserve since the Monthly Statment Account may not be initialized
                    ctx.accounts.signer.key().as_ref(),
                    &user_account_index.to_le_bytes().as_ref()],
                    &ctx.program_id
                );

                //Monthly Statement Account may not be initliazed if new month or user just hasn't done anything recently on this specific Token Sub Market
                if lending_user_monthly_statement_account_serialized.data_len() == 0
                {
                    let month_binding = lending_protocol.current_statement_month.to_le_bytes();
                    let current_statement_month_le_bytes_ref = month_binding.as_ref();
                    let year_binding = lending_protocol.current_statement_year.to_le_bytes();
                    let current_statement_year_le_bytes_ref = year_binding.as_ref();
                    let signer_binding = ctx.accounts.signer.key();
                    let signer_ref = signer_binding.as_ref();
                    let user_account_index_binding = user_account_index.to_le_bytes();
                    let user_account_index_le_ref = user_account_index_binding.as_ref();

                    let seeds: &[&[u8]] =
                    &[
                        b"userMonthlyStatementAccount",
                        current_statement_month_le_bytes_ref,
                        current_statement_year_le_bytes_ref,
                        token_reserve.token_mint_address.as_ref(),
                        signer_ref,
                        user_account_index_le_ref,
                        &[user_monthly_statement_account_bump]
                    ];

                    let account_space = 8 + std::mem::size_of::<LendingUserMonthlyStatementAccount>();

                    //CPI to create the account
                    anchor_lang::solana_program::program::invoke_signed(
                        &anchor_lang::solana_program::system_instruction::create_account(
                            &ctx.accounts.signer.key(),
                            &lending_user_monthly_statement_account_serialized.key(),
                            anchor_lang::solana_program::sysvar::rent::Rent::get()?.minimum_balance(account_space),
                            account_space as u64,
                            ctx.program_id,
                        ),
                        &[
                            ctx.accounts.signer.to_account_info(),
                            lending_user_monthly_statement_account_serialized.clone(),
                            ctx.accounts.system_program.to_account_info(),
                            ctx.accounts.rent.to_account_info(),
                        ],
                        &[seeds]//Seeds since this is a PDA and we want other functions to be able to update this Monthly Statement Account if it is initialized in this function that uses remaining accounts.
                    )?;
                    
                    initialize_lending_user_monthly_statement_account(
                        &mut lending_user_monthly_statement_account,
                        lending_protocol,
                        ctx.accounts.signer.key(),
                        user_account_index,
                        token_reserve.token_mint_address.key()
                    )?;
                }

                //Validate Sub Market, User Tab, and User Monthly Statement Accounts
                require_keys_eq!(expected_sub_market_pda.key(), sub_market_account_serialized.key(), InvalidInputError::UnexpectedTabAccount);
                require_keys_eq!(expected_lending_user_tab_account_pda.key(), lending_user_tab_account_serialized.key(), InvalidInputError::UnexpectedTabAccount);
                require_keys_eq!(expected_user_monthly_statement_account_pda.key(), lending_user_monthly_statement_account_serialized.key(), InvalidInputError::UnexpectedTabAccount);

                //You must provide all of the sub user's tab accounts ordered by user_tab_account_index
                require!(user_tab_index == lending_user_tab_account.user_tab_account_index, InvalidInputError::IncorrectOrderOfTabAccounts);

                update_user_previous_interest_earned(
                    &mut token_reserve,
                    &mut sub_market,
                    &mut lending_user_tab_account,
                    &mut lending_user_monthly_statement_account
                )?;

                update_user_previous_interest_accrued(
                    &mut token_reserve,
                    &mut sub_market,
                    &mut lending_user_tab_account,
                    &mut lending_user_monthly_statement_account
                )?;

                //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest index
                update_token_reserve_rates(&mut token_reserve)?;
                lending_user_tab_account.interest_change_index = token_reserve.interest_change_index;
                lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

                user_tab_index += 1;
            }
        }

        //You must provide all of the sub user's Tab Accounts in remaining accounts. Every Tab Account should have a corresponding Sub Market Account before it and Monthly Statement Account after it.(Token Reserve should be mentioned once at the beginning of the sequences)
        require!(user_lending_account.tab_account_count == user_tab_index, InvalidInputError::IncorrectNumberOfTabAccounts);

        //Stat Listener
        lending_stats.snap_shots += 1;

        msg!("Snap Shots updated for address: {}, user index: {}", ctx.accounts.signer.key(), user_account_index);

        Ok(())
    }

    pub fn claim_sub_market_fees(ctx: Context<ClaimSubMarketFees>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8
    ) -> Result<()> 
    {
        let sub_market = &mut ctx.accounts.sub_market;
        //Only the Fee Collector can call this function
        require_keys_eq!(ctx.accounts.signer.key(), sub_market.fee_collector_address.key(), AuthorizationError::NotFeeCollector);

        let token_reserve = &mut ctx.accounts.token_reserve;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                ctx.accounts.signer.key(),
                user_account_index,
                token_mint_address.key()
            )?;
        }

        //Calculate Token Reserve Previously Earned Interest
        update_token_reserve_accrued_interest_index(token_reserve, time_stamp)?;

        update_user_previous_interest_earned(
            token_reserve,
            sub_market,
            lending_user_tab_account,
            lending_user_monthly_statement_account
        )?;

        update_user_previous_interest_accrued(
            token_reserve,
            sub_market,
            lending_user_tab_account,
            lending_user_monthly_statement_account
        )?;

        //Collect Fees
        lending_user_tab_account.deposited_amount = lending_user_tab_account.deposited_amount + sub_market.uncollected_fees_amount;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
        lending_user_monthly_statement_account.snap_shot_fees_collected_amount = lending_user_tab_account.fees_collected_amount;
        lending_user_monthly_statement_account.monthly_fees_collected_amount = sub_market.uncollected_fees_amount;

        sub_market.uncollected_fees_amount = 0;

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest index
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.interest_change_index = token_reserve.interest_change_index;

        //Stat Listener
        lending_stats.fee_collections += 1;

        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp; //This gets set when calling deposit_tokens, repay_tokens, update_user_snap_shots, or claim_sub_market_fees
        
        msg!("Fees Collected for TokenReserve: {}, SubMarketOwner: {}, SubMarketIndex: {}, FeeCollector: {}, FeeCollectorAccountIndex: {}",
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index,
        ctx.accounts.signer.key(),
        user_account_index);

        Ok(())
    }
}

//Derived Accounts
#[derive(Accounts)]
pub struct InitializeLendingProtocol<'info> 
{
    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingProtocol".as_ref()],
        bump,
        space = size_of::<LendingProtocol>() + 8)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump,
        space = size_of::<LendingProtocolCEO>() + 8)]
    pub ceo: Account<'info, LendingProtocolCEO>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"tokenReserveStats".as_ref()],
        bump,
        space = size_of::<TokenReserveStats>() + 8)]
    pub token_reserve_stats: Account<'info, TokenReserveStats>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"subMarketStats".as_ref()],
        bump,
        space = size_of::<SubMarketStats>() + 8)]
    pub sub_market_stats: Account<'info, SubMarketStats>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingStats".as_ref()],
        bump,
        space = size_of::<LendingStats>() + 8)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingUserStats".as_ref()],
        bump,
        space = size_of::<LendingUserStats>() + 8)]
    pub lending_user_stats: Account<'info, LendingUserStats>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct PassOnLendingProtocolCEO<'info> 
{
    #[account(
        mut,
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, LendingProtocolCEO>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}


#[derive(Accounts)]
pub struct UpdateCurrentStatementMonthAndYear<'info> 
{
    #[account(
        mut,
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, LendingProtocolCEO>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey)]
pub struct AddTokenReserve<'info> 
{
    #[account(
        mut,
        seeds = [b"tokenReserveStats".as_ref()],
        bump)]
    pub token_reserve_stats: Account<'info, TokenReserveStats>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, LendingProtocolCEO>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump, 
        space = size_of::<TokenReserve>() + 8)]
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(
        init, 
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = token_reserve)]
    pub token_reserve_ata: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey)]
pub struct UpdateTokenReserve<'info> 
{
    #[account(
        mut,
        seeds = [b"tokenReserveStats".as_ref()],
        bump)]
    pub token_reserve_stats: Account<'info, TokenReserveStats>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, LendingProtocolCEO>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_index: u16)]
pub struct CreateSubMarket<'info> 
{
    #[account(
        mut,
        seeds = [b"subMarketStats".as_ref()],
        bump)]
    pub sub_market_stats: Account<'info, SubMarketStats>,

    #[account(
        init,
        payer = signer,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), signer.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<SubMarket>() + 8)]
    pub sub_market: Account<'info, SubMarket>,

    //The Token Reserve must exist to create a SubMarket. Only the ceo can create a Token Reserve.
    #[account(
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_index: u16)]
pub struct EditSubMarket<'info> 
{
    #[account(
        mut,
        seeds = [b"subMarketStats".as_ref()],
        bump)]
    pub sub_market_stats: Account<'info, SubMarketStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), signer.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, SubMarket>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct DepositTokens<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, LendingStats>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), sub_market_owner_address.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(
        init_if_needed, //SOL has to be deposited as wSol and the user may or may not have a wSol account already.
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer
    )]
    pub user_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint_address,
        associated_token::authority = token_reserve
    )]
    pub token_reserve_ata: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

//The Lending User Account gets created with a deposit and you can edit the account name on it afterwards
#[derive(Accounts)]
#[instruction(user_account_index: u8)]
pub struct EditLendingUserAccountName<'info> 
{
    #[account(
        mut,
        seeds = [b"lendingUserStats".as_ref()],
        bump)]
    pub lending_user_stats: Account<'info, LendingUserStats>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct WithdrawTokens<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), sub_market_owner_address.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(
        init_if_needed, //SOL has to be withdrawn as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer
    )]
    pub user_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint_address,
        associated_token::authority = token_reserve
    )]
    pub token_reserve_ata: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct BorrowTokens<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), sub_market_owner_address.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(
        init_if_needed, //Init ATA account of token being borrowed if it doesn't exist for User
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer
    )]
    pub user_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint_address,
        associated_token::authority = token_reserve
    )]
    pub token_reserve_ata: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct RepayTokens<'info> 
{
     #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), sub_market_owner_address.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint_address,
        associated_token::authority = signer
    )]
    pub user_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint_address,
        associated_token::authority = token_reserve
    )]
    pub token_reserve_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(user_account_index: u8)]
pub struct UpdateUserSnapShots<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct ClaimSubMarketFees<'info> 
{
     #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), sub_market_owner_address.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, SubMarket>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

//Accounts
#[account]
pub struct LendingProtocolCEO
{
    pub address: Pubkey
}

#[account]
pub struct LendingProtocol
{
    pub current_statement_month: u8,
    pub current_statement_year: u32
}

#[account]
pub struct TokenReserveStats
{
    pub token_reserve_count: u32,
    pub token_reserves_updated_count: u128
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
    pub deposits: u128,
    pub withdrawals: u128,
    pub borrows: u128,
    pub repayments: u128,
    pub liquidations: u128,
    pub snap_shots: u128,
    pub fee_collections: u128,
    pub collateral_swaps: u128
}

#[account]
pub struct LendingUserStats
{
    pub name_change_count: u128
}

#[account]
pub struct TokenReserve
{
    pub token_reserve_protocol_index: u32,
    pub token_mint_address: Pubkey,
    pub token_decimal_amount: u8,
    pub pyth_feed_id: [u8; PYTH_FEED_ID_LEN],
    pub pyth_feed_address: Pubkey,
    pub supply_apy: u128,
    pub borrow_apy: u16,
    pub utilization_rate: u64,
    pub global_limit: u128,
    pub interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Supply User interest is earned from Borrow Users so that it can be proportionally distributed to Supply Users
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub fees_generated_amount: u128,
    pub borrowed_amount: u128,
    pub interest_accrued_amount: u128,
    pub repaid_debt_amount: u128,
    pub liquidated_amount: u128,
    pub last_lending_activity_amount: u128,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64
}

#[account]
pub struct SubMarket
{
    pub id: u32,
    pub owner: Pubkey,
    pub token_mint_address: Pubkey,
    pub sub_market_index: u16,
    pub fee_collector_address: Pubkey,
    pub fee_on_interest_earned_rate: u16,
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub fees_generated_amount: u128,
    pub uncollected_fees_amount: u128,
    pub borrowed_amount: u128,
    pub interest_accrued_amount: u128,
    pub repaid_debt_amount: u128,
    pub liquidated_amount: u128,
    pub last_lending_activity_amount: u128,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64
}

#[account]
pub struct LendingUserAccount //Giving the lending account an index to allow users to have multiple lending accounts if they so choose, so they don't have to use multiple wallets
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub account_name: String,
    pub lending_user_account_added: bool,
    pub tab_account_count: u32,
}

#[account]
pub struct LendingUserTabAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub token_mint_address: Pubkey,
    pub pyth_feed_id: [u8; PYTH_FEED_ID_LEN],
    pub pyth_feed_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub user_tab_account_index: u32,
    pub user_tab_account_added: bool,
    pub interest_change_index: u128, //This index is set to match the token reserve index after previously accured interest and debt is updated
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub fees_generated_amount: u128,
    pub fees_collected_amount: u128,
    pub borrowed_amount: u128,
    pub interest_accrued_amount: u128,
    pub repaid_debt_amount: u128,
    pub user_was_liquidated_amount: u128,
    pub interest_change_last_updated_time_stamp: u64
}

#[account]
pub struct LendingUserMonthlyStatementAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub token_mint_address: Pubkey,
    pub statement_month: u8,
    pub statement_year: u32,
    pub monthly_statement_account_added: bool,
    pub snap_shot_balance_amount: u128,//The snap_shot properties give a snapshot of the value of the Tab Account over its whole life time at the time it is updated
    pub snap_shot_interest_earned_amount: u128,
    pub snap_shot_fees_generated_amount: u128,
    pub snap_shot_fees_collected_amount: u128,
    pub snap_shot_debt_amount: u128,
    pub snap_shot_interest_accrued_amount: u128,
    pub snap_shot_repaid_debt_amount: u128,
    pub snap_shot_user_was_liquidated_amount: u128,
    pub monthly_deposited_amount: u128,//The monthly properties give the specific value changes for that specific month
    pub monthly_interest_earned_amount: u128,
    pub monthly_fees_generated_amount: u128,
    pub monthly_fees_collected_amount: u128,
    pub monthly_withdrawal_amount: u128,
    pub monthly_borrowed_amount: u128,
    pub monthly_interest_accrued_amount: u128,
    pub monthly_repaid_debt_amount: u128,
    pub monthly_user_was_liquidated_amount: u128,
    pub last_lending_activity_amount: u128,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64 
}