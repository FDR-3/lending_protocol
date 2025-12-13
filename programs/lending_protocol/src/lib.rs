use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, SyncNative, CloseAccount};
use anchor_lang::system_program::{self};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;
use ra_solana_math::FixedPoint;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

declare_id!("5cAHP93tbTEwTBs6Xr3AGu6fCrkrwP9BRy7YCNXKsaqP");

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
    #[msg("Unexpected Token Reserve Account PDA detected")]
    UnexpectedTokenReserveAccount,
    #[msg("Unexpected SubMarket Account PDA detected")]
    UnexpectedSubMarketAccount,
    #[msg("Unexpected Monthly Statement Account PDA detected")]
    UnexpectedMonthlyStatementAccount,
    #[msg("Lending User Account name can't be longer than 25 characters")]
    LendingUserAccountNameTooLong
}

#[error_code]
pub enum LendingError
{
    #[msg("The price data was stale")]
    StalePriceData,
    #[msg("The Lending User Snap Shot data was stale")]
    StaleSnapShotData,
    #[msg("You can't withdraw or borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral")]
    LiquidationExposure,
    #[msg("You can't withdraw more funds than you've deposited")]
    InsufficientFunds,
    #[msg("Not enough liquidity in the Token Reserve for this withdraw or borrow")]
    InsufficientLiquidity,
    #[msg("You can't pay back more funds than you've borrowed")]
    TooManyFunds
}

//Helper function to update Token Reserve Accrued Interest Index before a lending transaction (deposit, withdraw, borrow, repay, liquidate)
//This function helps determine how much compounding interest a Token Reserve has earned for its token over the whole life of the Token Reserve's entire existence
fn update_token_reserve_supply_and_borrow_interest_change_index<'info>(token_reserve: &mut Account<TokenReserve>, new_lending_activity_time_stamp: u64) -> Result<()>
{
    //Skip if there is no borrowing in the Token Reserve
    if token_reserve.borrowed_amount == 0
    {
        return Ok(())
    }

    //Use ra_solana_math library FixedPoint for fixed point math
    let old_supply_interest_index_fixed_point = FixedPoint::from_int(token_reserve.supply_interest_change_index as u64);
    let old_borrow_interest_index_fixed_point = FixedPoint::from_int(token_reserve.borrow_interest_change_index as u64);
    let number_one_fixed_point = FixedPoint::from_int(1);
    let supply_apy_fixed_point = FixedPoint::from_bps(token_reserve.supply_apy as u64)?;
    let borrow_apy_fixed_point = FixedPoint::from_bps(token_reserve.borrow_apy as u64)?;
    let change_in_time = new_lending_activity_time_stamp - token_reserve.last_lending_activity_time_stamp;
    let change_in_time_fixed_point =  FixedPoint::from_int(change_in_time);
    let seconds_in_a_year_fixed_point = FixedPoint::from_int(31_556_952); //1 year = (365.2425 days) × (24 hours/day) × (3600 seconds/hour) = 31,556,952 seconds
    
    //Set Token Reserve Supply Interest Index = Old Supply Interest Index * (1 + Supply APY * Δt/Seconds in a Year)
    //Multiple before dividing to help keep precision
    let supply_apy_mul_change_in_time_fixed_point = supply_apy_fixed_point.mul(&change_in_time_fixed_point)?;
    let interest_change_factor_fixed_point = supply_apy_mul_change_in_time_fixed_point.div(&seconds_in_a_year_fixed_point)?;
    let one_plus_interest_change_factor_fixed_point = number_one_fixed_point.add(&interest_change_factor_fixed_point)?;
    let new_supply_interest_index_fixed_point = old_supply_interest_index_fixed_point.mul(&one_plus_interest_change_factor_fixed_point)?;
    let new_supply_interest_index = new_supply_interest_index_fixed_point.to_u128()?;
    token_reserve.supply_interest_change_index = new_supply_interest_index;

    //Set Token Reserve Borrow Interest Index = Old Borrow Interest Index * (1 + Borrow APY * Δt/Seconds in a Year)
    //Multiple before dividing to help keep precision
    let borrow_apy_mul_change_in_time_fixed_point = borrow_apy_fixed_point.mul(&change_in_time_fixed_point)?;
    let interest_change_factor_fixed_point = borrow_apy_mul_change_in_time_fixed_point.div(&seconds_in_a_year_fixed_point)?;
    let one_plus_interest_change_factor_fixed_point = number_one_fixed_point.add(&interest_change_factor_fixed_point)?;
    let new_borrow_interest_index_fixed_point = old_borrow_interest_index_fixed_point.mul(&one_plus_interest_change_factor_fixed_point)?;
    let new_borrow_interest_index = new_borrow_interest_index_fixed_point.to_u128()?;
    token_reserve.borrow_interest_change_index = new_borrow_interest_index;

    msg!("Updated Token Reserve Interest Change Indexes");
    msg!("Supply Change Index: {}", token_reserve.supply_interest_change_index);
    msg!("Borrow Change Index: {}", token_reserve.borrow_interest_change_index);

    Ok(())
}

//Helper function to update Token Reserve Utilization Rate and Supply Apy after a lending transaction (deposit, withdraw, borrow, repay, liquidate)
fn update_token_reserve_rates<'info>(token_reserve: &mut Account<TokenReserve>) -> Result<()>
{
    if token_reserve.borrowed_amount == 0
    {
        token_reserve.utilization_rate = 0;
        token_reserve.supply_apy = 0; //There can be no supply apy if no one is borrowing
    }
    else
    {
        //Borrow, Supply, and Utililzation rate stored as normal basis points, IE 101 basis points = 1.01%
        let decimal_scaling = 10_000; //10_000 = 100.00%

        //Set Token Reserve Utilization Rate = Borrowed Amount / Deposited Amount
        let borrowed_amount_scaled = token_reserve.borrowed_amount * decimal_scaling;
        let utilization_rate = borrowed_amount_scaled / token_reserve.deposited_amount;
        token_reserve.utilization_rate = utilization_rate as u16;

        //Set Supply APY = Borrowed APY * Utilization Rate
        let unscaled_supply_apy = token_reserve.borrow_apy as u32 * token_reserve.utilization_rate as u32;
        token_reserve.supply_apy = (unscaled_supply_apy / decimal_scaling as u32) as u16;
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

    //Use ra_solana_math library FixedPoint for fixed point math
    //User New Balance = Old Balance * Token Reserve Earned Interest Index / User Earned Interest Index
    let token_reserve_supply_index_fixed_point = FixedPoint::from_int(token_reserve.supply_interest_change_index as u64);
    let user_supply_index_fixed_point = FixedPoint::from_int(lending_user_tab_account.supply_interest_change_index as u64);
    let old_user_deposited_amount_fixed_point = FixedPoint::from_int(lending_user_tab_account.deposited_amount as u64);

    //Perform multiplication before division to help keep more precision
    let old_user_balance_mul_token_reserve_index_fixed_point = old_user_deposited_amount_fixed_point.mul(&token_reserve_supply_index_fixed_point)?;
    let new_user_deposited_amount_before_fee_fixed_point = old_user_balance_mul_token_reserve_index_fixed_point.div(&user_supply_index_fixed_point)?;
    let new_user_interest_earned_amount_before_fee_fixed_point = new_user_deposited_amount_before_fee_fixed_point.sub(&old_user_deposited_amount_fixed_point)?;
    
    //Apply SubMarket Fee
    let sub_market_fee_rate_fixed_point = FixedPoint::from_bps(sub_market.fee_on_interest_earned_rate as u64)?;
    let new_fees_generated_amount_fixed_point = new_user_interest_earned_amount_before_fee_fixed_point.mul(&sub_market_fee_rate_fixed_point)?;
    let new_fees_generated_amount = new_fees_generated_amount_fixed_point.to_u128()?;
    let new_user_interest_earned_amount_after_fee_fixed_point = new_user_interest_earned_amount_before_fee_fixed_point.sub(&new_fees_generated_amount_fixed_point)?;
    let new_user_interest_earned_amount_after_fee = new_user_interest_earned_amount_after_fee_fixed_point.to_u128()?;

    token_reserve.deposited_amount += new_user_interest_earned_amount_after_fee;
    token_reserve.interest_earned_amount += new_user_interest_earned_amount_after_fee;
    token_reserve.fees_generated_amount += new_fees_generated_amount;
    sub_market.deposited_amount += new_user_interest_earned_amount_after_fee;
    sub_market.interest_earned_amount += new_user_interest_earned_amount_after_fee;
    sub_market.fees_generated_amount += new_fees_generated_amount;
    sub_market.uncollected_fees_amount += new_user_interest_earned_amount_after_fee;
    lending_user_tab_account.deposited_amount += new_user_interest_earned_amount_after_fee;
    lending_user_tab_account.interest_earned_amount += new_user_interest_earned_amount_after_fee;
    lending_user_tab_account.fees_generated_amount += new_fees_generated_amount;
    lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
    lending_user_monthly_statement_account.snap_shot_interest_earned_amount = lending_user_tab_account.interest_earned_amount;
    lending_user_monthly_statement_account.snap_shot_fees_generated_amount = lending_user_tab_account.fees_generated_amount;
    lending_user_monthly_statement_account.monthly_interest_earned_amount += new_user_interest_earned_amount_after_fee;
    lending_user_monthly_statement_account.monthly_fees_generated_amount += new_fees_generated_amount;

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

    //Use ra_solana_math library FixedPoint for fixed point math
    //User New Debt = Old Debt * Token Reserve Accrued Interest Index / User Accrued Interest Index
    let token_reserve_borrow_index_fixed_point = FixedPoint::from_int(token_reserve.borrow_interest_change_index as u64);
    let user_borrow_index_fixed_point = FixedPoint::from_int(lending_user_tab_account.borrow_interest_change_index as u64);
    let old_user_borrowed_amount_fixed_point = FixedPoint::from_int(lending_user_tab_account.borrowed_amount as u64);

    //Perform multiplication before division to help keep more precision
    let old_user_debt_mul_token_reserve_index_fixed_point = old_user_borrowed_amount_fixed_point.mul(&token_reserve_borrow_index_fixed_point)?;
    let new_user_borrowed_amount_fixed_point = old_user_debt_mul_token_reserve_index_fixed_point.div(&user_borrow_index_fixed_point)?;
    let new_user_interest_accrued_amount_fixed_point = new_user_borrowed_amount_fixed_point.sub(&old_user_borrowed_amount_fixed_point)?;
    let new_user_interest_accrued_amount = new_user_interest_accrued_amount_fixed_point.to_u128()?;

    token_reserve.borrowed_amount += new_user_interest_accrued_amount;
    token_reserve.interest_accrued_amount += new_user_interest_accrued_amount;
    sub_market.borrowed_amount += new_user_interest_accrued_amount;
    sub_market.interest_accrued_amount += new_user_interest_accrued_amount;
    lending_user_tab_account.borrowed_amount += new_user_interest_accrued_amount;
    lending_user_tab_account.interest_accrued_amount += new_user_interest_accrued_amount;
    lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
    lending_user_monthly_statement_account.snap_shot_interest_accrued_amount = lending_user_tab_account.interest_accrued_amount;
    lending_user_monthly_statement_account.monthly_interest_accrued_amount += new_user_interest_accrued_amount;

    Ok(())
}

//Helper function to validate Tab Accounts and Pyth Price Update Accounts and to see if the Withdraw or Borrow request will lower the user's health factor below 30%
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
    let mut user_deposited_value = 0;
    let mut user_borrowed_value = 0;
    let mut user_withdraw_or_borrow_request_value = 0;
    let mut evaluated_price_of_withdraw_or_borrow_token = false;
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
        //2 minutes gives the user plenty of time to call both functions. Users shouldn't earn or accrue that much interest or debt within 2 minutes and if they do, that's what the liquidation function is for if there's an issue later :X
        let time_diff = new_lending_activity_time_stamp - tab_account.interest_change_last_updated_time_stamp;
        require!(time_diff <= 120, LendingError::StaleSnapShotData);
        
        //Validate Price Update Account
        let price_update_account_serialized = remaining_accounts_iter.next().unwrap(); //The Price Update Account comes after the Tab Account
        require_keys_eq!(tab_account.pyth_price_update_key.key(), price_update_account_serialized.key(), InvalidInputError::UnexpectedPythPriceUpdateAccount);

        let data_ref = price_update_account_serialized.data.borrow();
        let mut data_slice: &[u8] = data_ref.deref();

        let price_update_account = PriceUpdateV2::try_deserialize(&mut data_slice)?;
        
        /*msg!
        (
            "Time Stamp: {}",
            time_stamp
        );
        msg!
        (
            "Published Time: {}",
            price_update_account.price_message.publish_time
        );*/
        
        //The published time for the Pyth Price Update Account can be no older than 30 seconds
        let time_diff = (time_stamp - price_update_account.price_message.publish_time) as u64;
        //require!(time_diff <= MAXIMUM_PRICE_AGE, LendingError::StalePriceData);

        let current_price = price_update_account.price_message;

        /*msg!
        (
            "Token Price: {} +- {} x 10^{}",
            current_price.price,
            current_price.conf,
            current_price.exponent
        );*/

        user_deposited_value += tab_account.deposited_amount as i128 * current_price.price as i128;
        user_borrowed_value += tab_account.borrowed_amount as i128 * current_price.price as i128;

        //Only add the value of the token being withdrawn or borrowed once since there might be multiple SubMarkets
        if token_mint_address.key() == tab_account.token_mint_address.key() && evaluated_price_of_withdraw_or_borrow_token == false
        {
            user_withdraw_or_borrow_request_value += withdraw_or_borrow_amount as i128 * current_price.price as i128;
            evaluated_price_of_withdraw_or_borrow_token = true;

            msg!("Deposited Amount: {}", tab_account.deposited_amount);
            msg!("Requested Amount: {}", withdraw_or_borrow_amount);
        }

        user_tab_index += 1;
    }

    msg!
    (
        "Value calculation test. Deposited: {}, Borrowed: {}, Requested: {}",
        user_deposited_value,
        user_borrowed_value,
        user_withdraw_or_borrow_request_value
    );

    if activity_type == Activity::Withdraw as u8
    {
        user_deposited_value = user_deposited_value - user_withdraw_or_borrow_request_value as i128;
    }
    else
    {
        user_borrowed_value = user_borrowed_value + user_withdraw_or_borrow_request_value as i128;
    }

    if user_borrowed_value > 0
    {
        let seventy_percent_fixed_point = FixedPoint::from_percent(70)?;
        let user_deposited_value_fixed_point  = FixedPoint::from_int(user_deposited_value.try_into().unwrap());
        let seventy_percent_of_new_deposited_value_fixed_point = user_deposited_value_fixed_point.mul(&seventy_percent_fixed_point)?;
        let seventy_percent_of_new_deposited_value = seventy_percent_of_new_deposited_value_fixed_point.to_u128()? as i128;

        //You can't withdraw or borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral.
        require!(seventy_percent_of_new_deposited_value >= user_borrowed_value, LendingError::LiquidationExposure);
    }

    Ok(())
}

//Helper function to initialize Monthly Statement Accounts
fn initialize_lending_user_monthly_statement_account<'info>(lending_user_monthly_statement_account: &mut Account<LendingUserMonthlyStatementAccount>,
    lending_protocol: &Account<LendingProtocol>,
    token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    signer: Pubkey,
    user_account_index: u8
) -> Result<()>
{
    lending_user_monthly_statement_account.token_mint_address = token_mint_address;
    lending_user_monthly_statement_account.sub_market_owner_address = sub_market_owner_address;
    lending_user_monthly_statement_account.sub_market_index = sub_market_index;
    lending_user_monthly_statement_account.owner = signer.key();
    lending_user_monthly_statement_account.user_account_index = user_account_index;
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
    {
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
        pyth_price_update_key: Pubkey,
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
        token_reserve.pyth_price_update_key = pyth_price_update_key.key();
        token_reserve.borrow_apy = borrow_apy;
        token_reserve.global_limit = global_limit;
        token_reserve.supply_interest_change_index = 1_000_000_000_000_000_000;
        token_reserve.borrow_interest_change_index = 1_000_000_000_000_000_000;

        token_reserve.token_reserve_protocol_index = token_reserve_stats.token_reserve_count;
        token_reserve_stats.token_reserve_count += 1;

        msg!("Added Token Reserve #{}", token_reserve_stats.token_reserve_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
        msg!("Pyth PriceUpdate Account: {}", pyth_price_update_key);
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
        
        //Populate tab account if being newly initliazed. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            lending_user_tab_account.owner = ctx.accounts.signer.key();
            lending_user_tab_account.user_account_index = user_account_index;
            lending_user_tab_account.token_mint_address = token_mint_address;
            lending_user_tab_account.pyth_price_update_key = token_reserve.pyth_price_update_key;
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
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp)?;

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
                //Since the User has no other wrapped SOL, close the temporary wrapped SOL account
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

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount as u128;
        token_reserve.last_lending_activity_type = Activity::Deposit as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = amount as u128;
        sub_market.last_lending_activity_type = Activity::Deposit as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Deposit as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;

        msg!("{} deposited at token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);   

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
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        withdraw_max: bool
    ) -> Result<()> 
    {
        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let user_lending_account = &mut ctx.accounts.user_lending_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp)?;

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

        //After updating interest earned and accrued, set withdraw amount
        let withdraw_amount;

        if withdraw_max
        {
            withdraw_amount = lending_user_tab_account.deposited_amount as u64;
        }
        else
        {
            withdraw_amount = amount
        }

        //You can't withdraw more funds than you've deposited
        require!(lending_user_tab_account.deposited_amount >= withdraw_amount as u128, LendingError::InsufficientFunds);

        //You can't withdraw or borrow more funds than are currently available in the Token Reserve. This can happen if there is too much borrowing going on.
        let available_token_amount = token_reserve.deposited_amount - token_reserve.borrowed_amount;
        require!(available_token_amount >= withdraw_amount as u128, LendingError::InsufficientLiquidity);

        //You must provide all of the sub user's tab accounts in remaining accounts. Every Tab Account has a corresponding Pyth Price Update Account directly after it in the passed in array
        require!((user_lending_account.tab_account_count * 2) as usize == ctx.remaining_accounts.len() as usize, InvalidInputError::IncorrectNumberOfTabAndPythPriceUpdateAccounts);

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
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
            withdraw_amount,
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
        token::transfer(cpi_ctx, withdraw_amount)?;

        //Handle wSOL Token unwrap
        if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
        {
            let user_balance_after_transfer = ctx.accounts.user_ata.amount;

            if user_balance_after_transfer > withdraw_amount
            {
                //Since User already had wrapped SOL, only unwrapped the amount withdrawn
                let cpi_accounts = system_program::Transfer
                {
                    from: ctx.accounts.user_ata.to_account_info(),
                    to: ctx.accounts.signer.to_account_info()
                };
                let cpi_program = ctx.accounts.system_program.to_account_info();
                let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
                system_program::transfer(cpi_ctx, withdraw_amount)?;
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
        
        //Update Values and Stat Listener
        lending_stats.withdrawals += 1;
        sub_market.deposited_amount -= withdraw_amount as u128;
        token_reserve.deposited_amount -= withdraw_amount as u128;
        lending_user_tab_account.deposited_amount -= withdraw_amount as u128;
        lending_user_monthly_statement_account.monthly_withdrawal_amount += withdraw_amount as u128;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
        
        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = withdraw_amount as u128;
        token_reserve.last_lending_activity_type = Activity::Withdraw as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = withdraw_amount as u128;
        sub_market.last_lending_activity_type = Activity::Withdraw as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp; 
        lending_user_monthly_statement_account.last_lending_activity_amount = withdraw_amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Withdraw as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;
        
        msg!("{} withdrew at token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    pub fn borrow_tokens(ctx: Context<BorrowTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64
    ) -> Result<()> 
    {
        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let user_lending_account = &mut ctx.accounts.user_lending_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Populate tab account if being newly initliazed. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        //This is for when a user is borrowing a token they have never interacted with before
        if lending_user_tab_account.user_tab_account_added == false
        {
            lending_user_tab_account.owner = ctx.accounts.signer.key();
            lending_user_tab_account.user_account_index = user_account_index;
            lending_user_tab_account.token_mint_address = token_mint_address;
            lending_user_tab_account.pyth_price_update_key = token_reserve.pyth_price_update_key;
            lending_user_tab_account.sub_market_owner_address = sub_market_owner_address.key();
            lending_user_tab_account.sub_market_index = sub_market_index;
            lending_user_tab_account.user_tab_account_index = user_lending_account.tab_account_count;

            msg!("Created Lending User Tab Account Indexed At: {}", lending_user_tab_account.user_tab_account_index);
        }

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp)?;

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

        //You can't withdraw or borrow more funds than are currently available in the Token Reserve. This can happen if there is too much borrowing going on.
        let available_token_amount = token_reserve.deposited_amount - token_reserve.borrowed_amount;
        require!(available_token_amount >= amount as u128, LendingError::InsufficientLiquidity);

        //You must provide all of the sub user's tab accounts in remaining accounts. Every Tab Account has a corresponding Pyth Price Update Account directly after it in the passed in array
        require!((user_lending_account.tab_account_count * 2) as usize == ctx.remaining_accounts.len() as usize, InvalidInputError::IncorrectNumberOfTabAndPythPriceUpdateAccounts);

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

        //If lending user tab account was just initialized, set added to true and increase tab account after validation so it isn't included with the validation/liquidation calculations.
        if lending_user_tab_account.user_tab_account_added == false
        {
            lending_user_tab_account.user_tab_account_added = true;
            user_lending_account.tab_account_count += 1;
        }

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

        //Update Values and Stat Listener
        lending_stats.borrows += 1;
        sub_market.borrowed_amount += amount as u128;
        token_reserve.borrowed_amount += amount as u128;
        lending_user_tab_account.borrowed_amount += amount as u128;
        lending_user_monthly_statement_account.monthly_borrowed_amount += amount as u128;
        lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

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
        
        msg!("{} borrowed at token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    pub fn repay_tokens(ctx: Context<RepayTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
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
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp)?;

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

        //After updating interest earned and accrued, set payment amount
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
        require!(lending_user_tab_account.borrowed_amount >= payment_amount as u128, LendingError::TooManyFunds);

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
            system_program::transfer(cpi_ctx, payment_amount)?;

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
                //Since the User has no other wrapped SOL, close the temporary wrapped SOL account
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
            token::transfer(cpi_ctx, payment_amount)?;  
        }

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
        
        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = payment_amount as u128;
        token_reserve.last_lending_activity_type = Activity::Repay as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = payment_amount as u128;
        sub_market.last_lending_activity_type = Activity::Repay as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = payment_amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;
  
        msg!("{} repaid debt for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    //Updates the interest earned and accrued for a given user's tab account. The last calculation must be no older than 120 seconds when doing withdrawals or borrows and this function helps refresh them.
    //You have to call it on all user tab accounts to get them all updated
    pub fn update_user_snap_shot(ctx: Context<UpdateUserSnapShot>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8
    ) -> Result<()> 
    {
        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Initialize monthly statement account if the statement month/year has changed or brand new sub user account.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_protocol,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp)?;

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

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

        //UpdateStat Listener
        lending_stats.snap_shots += 1;

        //Update last activity on accounts
        token_reserve.last_lending_activity_time_stamp = time_stamp;

        msg!("Snap Shots updated for TokenMintAddress: {}, SubMarketOwner: {}, SubMarketIndex: {}", token_reserve.token_mint_address.key(), sub_market.owner.key(), sub_market_index);
        msg!("UserAddress: {}, UserAccountIndex: {}", ctx.accounts.signer.key(), user_account_index);

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

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
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
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp)?;

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

        //Update Token Reserve Supply APY and Global Utilization Rates and the User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.interest_change_last_updated_time_stamp = time_stamp;

        //Stat Listener
        lending_stats.fee_collections += 1;

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
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
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
    pub token_reserve_ata: Box<Account<'info, TokenAccount>>,

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
    pub lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
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
    pub lending_stats: Box<Account<'info, LendingStats>>,

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
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
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
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSol and the user may or may not have a wSol account already.
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
    pub token_reserve_ata: Box<Account<'info, TokenAccount>>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct UpdateUserSnapShot<'info> 
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
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Account<'info, TokenReserve>,

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
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
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
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, LendingStats>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

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
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
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
    pub pyth_price_update_key: Pubkey,
    pub supply_apy: u16,
    pub borrow_apy: u16,
    pub utilization_rate: u16,
    pub global_limit: u128,
    pub supply_interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Supply User interest is earned from Borrow Users so that it can be proportionally distributed to Supply Users
    pub borrow_interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Borrow User interest is accrued for Supply Users so that it can be proportionally distributed to Borrow Users
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
    pub token_mint_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub pyth_price_update_key: Pubkey,
    pub user_tab_account_index: u32,
    pub user_tab_account_added: bool,
    pub supply_interest_change_index: u128, //This index is set to match the token reserve index after previously earned interest is updated
    pub borrow_interest_change_index: u128, //This index is set to match the token reserve index after previously accured interest is updated
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
    pub token_mint_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub owner: Pubkey,
    pub user_account_index: u8,
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