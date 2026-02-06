use anchor_lang::prelude::*;
use anchor_lang::system_program::{self};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenInterface, TokenAccount, TransferChecked, SyncNative, CloseAccount};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;
use ra_solana_math::FixedPoint;
use pyth_solana_receiver_sdk::price_update::{Price, PriceUpdateV2};
use hex;
pub mod validation;
pub mod errors;
use crate::validation::*;
use crate::errors::*;

declare_id!("SYcBiQtCfjAia7DkkXYubztiQ1e5AGjKPKsM9iJz8od");

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
#[cfg(feature = "dev")] 
const INITIAL_SOLVENCY_TREASURER_ADDRESS: Pubkey = pubkey!("2TnxW9qAgPjHmHUXde6zgxNa8F4nY3kfDpdRJsT8HdPU");
#[cfg(feature = "dev")] 
const INITIAL_LIQUIDATION_TREASURER_ADDRESS: Pubkey = pubkey!("9BRgCdmwyP5wGVTvKAUDjSwucpqGncurVa35DjaWqSsC");
#[cfg(feature = "dev")]
use pyth_solana_receiver_sdk::ID as PYTH_RECEIVER_ID;
#[cfg(feature = "dev")]
const PYTH_PROGRAM_ID: Pubkey = PYTH_RECEIVER_ID;

#[cfg(feature = "local")] 
const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("DSLn1ofuSWLbakQWhPUenSBHegwkBBTUwx8ZY4Wfoxm");
#[cfg(feature = "local")] 
const INITIAL_SOLVENCY_TREASURER_ADDRESS: Pubkey = pubkey!("DSLn1ofuSWLbakQWhPUenSBHegwkBBTUwx8ZY4Wfoxm");
#[cfg(feature = "local")] 
const INITIAL_LIQUIDATION_TREASURER_ADDRESS: Pubkey = pubkey!("DSLn1ofuSWLbakQWhPUenSBHegwkBBTUwx8ZY4Wfoxm");
#[cfg(feature = "local")] 
const PYTH_PROGRAM_ID: Pubkey = pubkey!("9NjP3rtyDtirKvYMwuyrHigznPTCmjwgbmc9MLLJvtWd");

const SOL_TOKEN_MINT_ADDRESS: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

//Processed claims need atleast 3 extra bytes of space to pass with full load
const LENDING_USER_ACCOUNT_EXTRA_SIZE: usize = 4;

const MAX_ACCOUNT_NAME_LENGTH: usize = 25;

const BASE_10_INT :u128 = 10;

enum Activity
{
    Deposit = 0,
    Withdraw = 1,
    Borrow = 2,
    Repay = 3,
    Liquidate = 4,
    CollectSubMarketFees = 5,
    CollectSolvencyFees = 6,
    CollectLiquidationFees = 7
}

//Helper function to update Token Reserve Accrued Interest Index before a lending transaction (deposit, withdraw, borrow, repay, liquidate)
//This function helps determine how much compounding interest a Token Reserve has earned for its token over the Token Reserve's entire existence
fn update_token_reserve_supply_and_borrow_interest_change_index<'info>(token_reserve: &mut TokenReserve, new_time_stamp: u64, new_clock_slot: Option<u64>) -> Result<()>
{
    //Skip if there is no borrowing in the Token Reserve. There is no interest change if there is no borrowing.
    if token_reserve.borrowed_amount != 0
    {
        //Use ra_solana_math library FixedPoint for fixed point math
        let old_supply_interest_index_fp = FixedPoint::from_int(token_reserve.supply_interest_change_index as u64);
        let old_borrow_interest_index_fp = FixedPoint::from_int(token_reserve.borrow_interest_change_index as u64);
        let number_one_fp = FixedPoint::from_int(1);
        let supply_apy_fp = FixedPoint::from_bps(token_reserve.supply_apy as u64)?;
        let borrow_apy_fp = FixedPoint::from_bps(token_reserve.borrow_apy as u64)?;
        let change_in_time = new_time_stamp - token_reserve.last_lending_activity_time_stamp;
        let change_in_time_fp =  FixedPoint::from_int(change_in_time);
        let seconds_in_a_year_fp = FixedPoint::from_int(31_556_952); //1 year = (365.2425 days) × (24 hours/day) × (3600 seconds/hour) = 31,556,952 seconds
        
        //Set Token Reserve Supply Interest Index = Old Supply Interest Index * (1 + Supply APY * Δt/Seconds in a Year)
        //Multiply before dividing to help keep precision
        let supply_apy_mul_change_in_time_fp = supply_apy_fp.mul(&change_in_time_fp)?;
        let interest_change_factor_fp = supply_apy_mul_change_in_time_fp.div(&seconds_in_a_year_fp)?;
        let one_plus_interest_change_factor_fp = number_one_fp.add(&interest_change_factor_fp)?;
        let new_supply_interest_index_fp = old_supply_interest_index_fp.mul(&one_plus_interest_change_factor_fp)?;
        let new_supply_interest_index = new_supply_interest_index_fp.to_u128()?;
        token_reserve.supply_interest_change_index = new_supply_interest_index;

        //Set Token Reserve Borrow Interest Index = Old Borrow Interest Index * (1 + Borrow APY * Δt/Seconds in a Year)
        //Multiply before dividing to help keep precision
        let borrow_apy_mul_change_in_time_fp = borrow_apy_fp.mul(&change_in_time_fp)?;
        let interest_change_factor_fp = borrow_apy_mul_change_in_time_fp.div(&seconds_in_a_year_fp)?;
        let one_plus_interest_change_factor_fp = number_one_fp.add(&interest_change_factor_fp)?;
        let new_borrow_interest_index_fp = old_borrow_interest_index_fp.mul(&one_plus_interest_change_factor_fp)?;
        let new_borrow_interest_index = new_borrow_interest_index_fp.to_u128()?;
        token_reserve.borrow_interest_change_index = new_borrow_interest_index;

        msg!("Updated Token Reserve Interest Change Indexes");
        msg!("Supply Change Index: {}", token_reserve.supply_interest_change_index);
        msg!("Borrow Change Index: {}", token_reserve.borrow_interest_change_index);
    }

    token_reserve.last_lending_activity_time_stamp = new_time_stamp;

    if let Some(clock_slot) = new_clock_slot
    {
        token_reserve.last_health_update_clock_slot = clock_slot;
    }
    
    Ok(())
}

//Helper function to update Token Reserve Utilization Rate, Borrow APY, and Supply APY after a lending transaction (deposit, withdraw, borrow, repay, liquidate)
fn update_token_reserve_rates<'info>(token_reserve: &mut TokenReserve) -> Result<()>
{
    if token_reserve.borrowed_amount == 0
    {
        token_reserve.utilization_rate = 0;
        token_reserve.supply_apy = 0; //There can be no supply apy if no one is borrowing
        token_reserve.borrow_apy = token_reserve.fixed_borrow_apy;
    }
    else
    {
        //Borrow, Supply, and Utililzation rate stored as normal basis points, IE 101 basis points = 1.01%
        let decimal_scaling = 10_000; //10_000 = 100.00%

        //Set Token Reserve Utilization Rate = Borrowed Amount / Deposited Amount
        let borrowed_amount_scaled = token_reserve.borrowed_amount * decimal_scaling;
        let utilization_rate = borrowed_amount_scaled / token_reserve.deposited_amount;
        token_reserve.utilization_rate = utilization_rate as u16;

        //Set Borrow APY
        if token_reserve.use_fixed_borrow_apy
        {
            token_reserve.borrow_apy = token_reserve.fixed_borrow_apy;
        }
        else
        {
            let optimal_utilization_rate = 7_000; //7_000 = 70.00%
            let utilization_rate = token_reserve.utilization_rate as u128;
            
            //Borrow APY = Borrow APY Base + ((Utilization Rate/Optimal Utialization Rate) * Borrow APY Slope1)
            //Setting Borrow APY Base to Borrow APY Slope1 in this case
            if utilization_rate < optimal_utilization_rate
            {
                //Max Borrow Rate = token_reserve.fixed_borrow_apy + token_reserve.fixed_borrow_apy @Less Than 70% Utilization Rate
                let borrow_apy_slope1 = token_reserve.fixed_borrow_apy as u128;
                //Multiply before dividing to help keep precision
                let u_rate_times_borrow_apy_slope1 = utilization_rate * borrow_apy_slope1;
                let u_rate_times_borrow_apy_slope1_divide_optimal_u_rate = u_rate_times_borrow_apy_slope1 / optimal_utilization_rate;

                token_reserve.borrow_apy = (borrow_apy_slope1 + u_rate_times_borrow_apy_slope1_divide_optimal_u_rate) as u16;
            }
            //Borrow APY = Borrow APY Base + Borrow APY Slope1 + ((Utilization Rate - Optimal Utialization Rate) / (100% Utilization - Optimal Utialization Rate) * Borrow APY Slope2)
            //Not using a Borrow APY Base in this case
            else
            {
                //Max Borrow Rate = 10% + 100% = 110% @100% Utilization Rate. I think having a rate more than 110% would appear too pay day loany...just seems like a bad look lol.
                let ten_percent = 1_000; //1_000 = 10.00%
                let u_rate_minus_optimal_u_rate = utilization_rate - optimal_utilization_rate;
                let one_hundred_percent_minus_optimal_u_rate = decimal_scaling - optimal_utilization_rate;
                //Multiply before dividing to help keep precision
                let u_rate_minus_optimal_u_rate_times_borrow_apy_slope2 = u_rate_minus_optimal_u_rate * decimal_scaling;
                let new_high_rate_base = u_rate_minus_optimal_u_rate_times_borrow_apy_slope2 / one_hundred_percent_minus_optimal_u_rate;

                token_reserve.borrow_apy = (ten_percent + new_high_rate_base) as u16;
            }
        }

        //Set Supply APY = Borrowed APY * Utilization Rate
        let unscaled_supply_apy = token_reserve.borrow_apy as u32 * token_reserve.utilization_rate as u32;
        token_reserve.supply_apy = (unscaled_supply_apy / decimal_scaling as u32) as u16;
    }
    
    msg!("Updated Token Reserve Rates");
    msg!("Utilization Rate: {}", token_reserve.utilization_rate as f64 / 100.0);
    msg!("Supply Apy: {}", token_reserve.supply_apy as f64 / 100.0);

    Ok(())
}

//Helper function to update User Interest Earned amounts. Also updates deposit amounts on the Token Reserve, SubMarket, and user Monthly Statement
fn update_user_previous_interest_earned<'info>(
    token_reserve: &mut TokenReserve,
    sub_market: &mut SubMarket,
    lending_user_tab_account: &mut LendingUserTabAccount,
    lending_user_monthly_statement_account: &mut LendingUserMonthlyStatementAccount
) -> Result<()>
{
    //Skip if the user has no deposited amount
    if lending_user_tab_account.deposited_amount == 0
    {
        return Ok(())
    }

    //Use ra_solana_math library FixedPoint for fixed point math
    //User New Balance = Old Balance * Token Reserve Earned Interest Index / User Earned Interest Index
    let token_reserve_supply_index_fp = FixedPoint::from_int(token_reserve.supply_interest_change_index as u64);
    let user_supply_index_fp = FixedPoint::from_int(lending_user_tab_account.supply_interest_change_index as u64);
    let old_user_deposited_amount_fp = FixedPoint::from_int(lending_user_tab_account.deposited_amount as u64);
    //let round_up_at_point_5 = FixedPoint::from_ratio(1, 2)?;//Add 0.5 before floor() or to_128() when rounding

    //Perform multiplication before division to help keep more precision
    let old_user_balance_mul_token_reserve_index_fp = old_user_deposited_amount_fp.mul(&token_reserve_supply_index_fp)?;
    let new_user_deposited_amount_before_fees_fp = old_user_balance_mul_token_reserve_index_fp.div(&user_supply_index_fp)?;
    let new_user_interest_earned_amount_before_fees_fp = new_user_deposited_amount_before_fees_fp.sub(&old_user_deposited_amount_fp)?;

    //Make Sure SubMarket Fee and Solvency Insurance Fee don't exceed 100%
    let sub_market_fee;
    let solvency_insurance_fee;
    if sub_market.fee_on_interest_earned_rate + token_reserve.solvency_insurance_fee_rate <= 10_000
    {
        sub_market_fee = sub_market.fee_on_interest_earned_rate;
        solvency_insurance_fee = token_reserve.solvency_insurance_fee_rate;
    }
    else
    {
        solvency_insurance_fee = token_reserve.solvency_insurance_fee_rate;
        sub_market_fee = 10_000 - token_reserve.solvency_insurance_fee_rate;
    }
   
    //Calculate Total Fee
    //The separate fee approach (below this commented out total fee approach) keeps the fees symmertrical always when they are the same rate and is more consistent
    //IE: Total fee is 1.92 so submarket fee(example rate 4%) is 1 and solvency fee(example rate 4%) is 0.
    /*let total_fee_rate_fp = FixedPoint::from_bps((sub_market_fee + solvency_insurance_fee)as u64)?;
    let total_fees_generated_fp_floor = ((new_user_interest_earned_amount_before_fees_fp.mul(&total_fee_rate_fp)?)).floor(); //Taking the floor before subtraction prevents the token reserve from having extra deposit amounts. Although having an extra deposit amount can act as a safety buffer for liquidity when there is bad debt, that's what the solvency insurance fee is for.

    //Calculate Solvency Insurance Fee
    let solvency_insurance_ratio_fp = FixedPoint::from_bps(solvency_insurance_fee as u64)?.div(&total_fee_rate_fp)?; //Get Solvency percentage of Fees
    let new_solvency_insurance_fees_generated_amount_fp_floor = total_fees_generated_fp_floor.mul(&solvency_insurance_ratio_fp)?.floor();
    let new_solvency_insurance_fees_generated_amount = new_solvency_insurance_fees_generated_amount_fp_floor.to_u128()?;

    //Calculate SubMarket Fee
    let new_sub_market_fees_generated_amount_fp = total_fees_generated_fp_floor.sub(&new_solvency_insurance_fees_generated_amount_fp_floor)?; //Submarket fee is the remainder without taking the floor again
    let new_sub_market_fees_generated_amount = new_sub_market_fees_generated_amount_fp.to_u128()?;

    //Apply Fees to Interest Earned
    let new_user_interest_earned_amount_after_fees_fp = new_user_interest_earned_amount_before_fees_fp.sub(&total_fees_generated_fp_floor)?;
    let new_user_interest_earned_amount_after_fees = new_user_interest_earned_amount_after_fees_fp.to_u128()?;*/

    //Separate Fee Approach
    //Calculate SubMarket Fee
    let sub_market_fee_rate_fp = FixedPoint::from_bps(sub_market_fee as u64)?;
    let new_sub_market_fees_generated_amount_before_round = new_user_interest_earned_amount_before_fees_fp.mul(&sub_market_fee_rate_fp)?; //Taking the floor before subtraction prevents the token reserve from having extra deposit amounts. Although having an extra deposit amount can act as a safety buffer for liquidity when there is bad debt, that's what the solvency insurance fee is for.
    let new_sub_market_fees_generated_amount_fp_floor = (new_sub_market_fees_generated_amount_before_round/*.add(&round_up_at_point_5)?*/).floor();
    let new_sub_market_fees_generated_amount = new_sub_market_fees_generated_amount_fp_floor.to_u128()?;

    //Calculate Solvency Insurance Fee
    let solvency_insurance_fee_rate_fp = FixedPoint::from_bps(solvency_insurance_fee as u64)?;
    let new_solvency_insurance_fees_generated_amount_before_round = new_user_interest_earned_amount_before_fees_fp.mul(&solvency_insurance_fee_rate_fp)?; //Taking the floor before subtraction prevents the token reserve from having extra deposit amounts. Although having an extra deposit amount can act as a safety buffer for liquidity when there is bad debt, that's what the solvency insurance fee is for.
    let new_solvency_insurance_fees_generated_amount_fp_floor = (new_solvency_insurance_fees_generated_amount_before_round/*.add(&round_up_at_point_5)?*/).floor();
    let mut new_solvency_insurance_fees_generated_amount = new_solvency_insurance_fees_generated_amount_fp_floor.to_u128()?;

    //Apply Fees to Interest Earned
    let new_user_interest_earned_amount_after_sb_fee_fp = new_user_interest_earned_amount_before_fees_fp.sub(&new_sub_market_fees_generated_amount_fp_floor)?;
    let new_user_interest_earned_amount_after_fees_fp = new_user_interest_earned_amount_after_sb_fee_fp.sub(&new_solvency_insurance_fees_generated_amount_fp_floor)?;
    let mut new_user_interest_earned_amount_after_fees = new_user_interest_earned_amount_after_fees_fp.to_u128()?;

    //User should earn 0% interest when combine fee rates are 100%
    //Due to the separate fee operations above, 'new_user_interest_earned_amount_after_fees' might still hold 1 dust.
    if sub_market_fee + solvency_insurance_fee == 10_000 && new_user_interest_earned_amount_after_fees > 0
    {
        //Sweep the remaining dust into Solvency
        new_solvency_insurance_fees_generated_amount += new_user_interest_earned_amount_after_fees;
        new_user_interest_earned_amount_after_fees = 0;
    }
    
    token_reserve.deposited_amount += new_user_interest_earned_amount_after_fees;
    token_reserve.interest_earned_amount += new_user_interest_earned_amount_after_fees;
    token_reserve.sub_market_fees_generated_amount += new_sub_market_fees_generated_amount;
    token_reserve.solvency_insurance_fees_generated_amount += new_solvency_insurance_fees_generated_amount;
    token_reserve.uncollected_solvency_insurance_fees_amount += new_solvency_insurance_fees_generated_amount;
    sub_market.deposited_amount += new_user_interest_earned_amount_after_fees;
    sub_market.interest_earned_amount += new_user_interest_earned_amount_after_fees;
    sub_market.sub_market_fees_generated_amount += new_sub_market_fees_generated_amount;
    sub_market.uncollected_sub_market_fees_amount += new_sub_market_fees_generated_amount;
    sub_market.solvency_insurance_fees_generated_amount += new_solvency_insurance_fees_generated_amount;
    lending_user_tab_account.deposited_amount += new_user_interest_earned_amount_after_fees as u64;
    lending_user_tab_account.interest_earned_amount += new_user_interest_earned_amount_after_fees as u64;
    lending_user_tab_account.sub_market_fees_generated_amount += new_sub_market_fees_generated_amount as u64;
    lending_user_tab_account.solvency_insurance_fees_generated_amount += new_solvency_insurance_fees_generated_amount as u64;
    lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
    lending_user_monthly_statement_account.snap_shot_interest_earned_amount = lending_user_tab_account.interest_earned_amount;
    lending_user_monthly_statement_account.snap_shot_sub_market_fees_generated_amount = lending_user_tab_account.sub_market_fees_generated_amount;
    lending_user_monthly_statement_account.snap_shot_solvency_insurance_fees_generated_amount = lending_user_tab_account.solvency_insurance_fees_generated_amount;
    lending_user_monthly_statement_account.monthly_interest_earned_amount += new_user_interest_earned_amount_after_fees as u64;
    lending_user_monthly_statement_account.monthly_sub_market_fees_generated_amount += new_sub_market_fees_generated_amount as u64;
    lending_user_monthly_statement_account.monthly_solvency_insurance_fees_generated_amount += new_solvency_insurance_fees_generated_amount as u64;

    Ok(())
}

//Helper function to update User Accured Debt amounts. Also updates debt amounts on the Token Reserve, SubMarket, and user Monthly Statement
fn update_user_previous_interest_accrued<'info>(
    token_reserve: &mut TokenReserve,
    sub_market: &mut SubMarket,
    lending_user_tab_account: &mut LendingUserTabAccount,
    lending_user_monthly_statement_account: &mut LendingUserMonthlyStatementAccount
) -> Result<()>
{
    //Skip if the user has no borrowed amount
    if lending_user_tab_account.borrowed_amount == 0
    {
        return Ok(())
    }

    //Use ra_solana_math library FixedPoint for fixed point math
    //User New Debt = Old Debt * Token Reserve Accrued Interest Index / User Accrued Interest Index
    let token_reserve_borrow_index_fp = FixedPoint::from_int(token_reserve.borrow_interest_change_index as u64);
    let user_borrow_index_fp = FixedPoint::from_int(lending_user_tab_account.borrow_interest_change_index as u64);
    let old_user_borrowed_amount_fp = FixedPoint::from_int(lending_user_tab_account.borrowed_amount as u64);

    //Perform multiplication before division to help keep more precision
    let old_user_debt_mul_token_reserve_index_fp = old_user_borrowed_amount_fp.mul(&token_reserve_borrow_index_fp)?;
    let new_user_borrowed_amount_fp = old_user_debt_mul_token_reserve_index_fp.div(&user_borrow_index_fp)?;
    let new_user_interest_accrued_amount_fp = (new_user_borrowed_amount_fp.sub(&old_user_borrowed_amount_fp)?).ceil()?;
    let new_user_interest_accrued_amount = new_user_interest_accrued_amount_fp.to_u128()?;

    token_reserve.borrowed_amount += new_user_interest_accrued_amount;
    token_reserve.interest_accrued_amount += new_user_interest_accrued_amount;
    sub_market.borrowed_amount += new_user_interest_accrued_amount;
    sub_market.interest_accrued_amount += new_user_interest_accrued_amount;
    lending_user_tab_account.borrowed_amount += new_user_interest_accrued_amount as u64;
    lending_user_tab_account.interest_accrued_amount += new_user_interest_accrued_amount as u64;
    lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
    lending_user_monthly_statement_account.snap_shot_interest_accrued_amount = lending_user_tab_account.interest_accrued_amount;
    lending_user_monthly_statement_account.monthly_interest_accrued_amount += new_user_interest_accrued_amount as u64;

    Ok(())
}

fn get_token_pyth_usd_value<'info>(price_update_account_serialized: &AccountInfo<'info>, pyth_feed_id: [u8; 32]) -> Result<u128>
{
    //Validate Price Update Account
    require_keys_eq!(*price_update_account_serialized.owner, PYTH_PROGRAM_ID, InvalidInputError::UnexpectedPythPriceUpdateAccount);

    let mut data_slice: &[u8] = &price_update_account_serialized.data.borrow();

    let price_update_account = PriceUpdateV2::try_deserialize(&mut data_slice)?;
    let current_price = get_token_pyth_price_by_id(price_update_account, pyth_feed_id)?;

    //Negative Price Detected
    require!(current_price.price > 0, LendingError::NegativePriceDetected);

    //Oracle Price Too Unstable
    let uncertainty_ratio = current_price.conf as f64 / current_price.price as f64;
    require!(uncertainty_ratio <= 0.02, LendingError::OraclePriceTooUnstable);//Reject price if more than 2% price uncertainty
    
    let normalized_price_8_decimals = normalize_pyth_price_to_8_decimals(current_price.price, current_price.exponent);

    //Debug
    /*msg!
    (
        "Token Price: {} +- {} x 10^{}",
        current_price.price,
        current_price.conf,
        current_price.exponent
    );*/

    Ok(normalized_price_8_decimals)
}

//Helper function to get the token price by the pyth ID
fn get_token_pyth_price_by_id<'info>(price_update_account: PriceUpdateV2, pyth_feed_id: [u8; 32]) -> Result<Price>
{
    pub const MAXIMUM_AGE: u64 = 30; //30 seconds

    let current_price: Price = price_update_account
    .get_price_no_older_than(
        &Clock::get()?, 
        MAXIMUM_AGE, 
        &pyth_feed_id
    )
    .map_err(|_| error!(LendingError::StalePriceDataOrWrongFeedID))?; //Handle Option returned by pyth (None if stale or wrong feed)

    Ok(current_price)
}

fn normalize_pyth_price_to_8_decimals(pyth_price: i64, pyth_expo: i32) -> u128
{
    let expo = pyth_expo.abs() as u32;

    if expo > 8
    {
        let conversion_number = BASE_10_INT.pow(expo - 8); 
        return pyth_price as u128 / conversion_number;
    }
    else if expo < 8
    {
        let conversion_number = BASE_10_INT.pow(8 - expo); 
        return pyth_price as u128 * conversion_number;
    }
    else
    {
        return pyth_price as u128;
    }
}

//Helper function to initialize Lending User Account
fn initialize_lending_user_account<'info>(lending_user_account: &mut LendingUserAccount,
    user_account_owner: Pubkey,
    user_account_index: u8,
    account_name: String
) -> Result<()>
{
    //Account Name string must not be longer than 25 characters
    require!(account_name.len() <= MAX_ACCOUNT_NAME_LENGTH, InvalidInputError::LendingUserAccountNameTooLong);

    lending_user_account.owner = user_account_owner;
    lending_user_account.user_account_index = user_account_index;
    lending_user_account.account_name = account_name.clone();
    lending_user_account.lending_user_account_added = true;

    msg!("Created Lending User Account Named: {}", account_name);

    Ok(())
}

//Helper function to initialize Lending User Tab Account
fn initialize_lending_user_tab_account<'info>(lending_user_account: &mut LendingUserAccount,
    lending_user_tab_account: &mut LendingUserTabAccount,
    bump: u8,
    token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner: Pubkey,
    user_account_index: u8
) -> Result<()>
{
    lending_user_tab_account.bump = bump;
    lending_user_tab_account.token_mint_address = token_mint_address;
    lending_user_tab_account.sub_market_owner_address = sub_market_owner_address;
    lending_user_tab_account.sub_market_index = sub_market_index;
    lending_user_tab_account.user_tab_account_index = lending_user_account.tab_account_count;
    lending_user_tab_account.owner = user_account_owner;
    lending_user_tab_account.user_account_index = user_account_index;
    lending_user_tab_account.user_tab_account_added = true;

    lending_user_account.tab_account_count += 1;

    msg!("Created Lending User Tab Account Indexed At: {}", lending_user_tab_account.user_tab_account_index);

    Ok(())
}

//Helper function to initialize Monthly Statement Account
fn initialize_lending_user_monthly_statement_account<'info>(lending_user_monthly_statement_account: &mut LendingUserMonthlyStatementAccount,
    lending_user_tab_account: &LendingUserTabAccount,
    lending_protocol: &LendingProtocol,
    bump: u8,
    token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner: Pubkey,
    user_account_index: u8
) -> Result<()>
{
    lending_user_monthly_statement_account.bump = bump;
    lending_user_monthly_statement_account.token_mint_address = token_mint_address;
    lending_user_monthly_statement_account.sub_market_owner_address = sub_market_owner_address;
    lending_user_monthly_statement_account.sub_market_index = sub_market_index;
    lending_user_monthly_statement_account.owner = user_account_owner;
    lending_user_monthly_statement_account.user_account_index = user_account_index;
    lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
    lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
    lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
    lending_user_monthly_statement_account.snap_shot_interest_earned_amount = lending_user_tab_account.interest_earned_amount;
    lending_user_monthly_statement_account.snap_shot_sub_market_fees_generated_amount = lending_user_tab_account.sub_market_fees_generated_amount;
    lending_user_monthly_statement_account.snap_shot_sub_market_fees_collected_amount = lending_user_tab_account.sub_market_fees_collected_amount;
    lending_user_monthly_statement_account.snap_shot_solvency_insurance_fees_generated_amount = lending_user_tab_account.solvency_insurance_fees_generated_amount;
    lending_user_monthly_statement_account.snap_shot_solvency_insurance_fees_collected_amount = lending_user_tab_account.solvency_insurance_fees_collected_amount;
    lending_user_monthly_statement_account.snap_shot_liquidation_fees_generated_amount = lending_user_tab_account.liquidation_fees_generated_amount;
    lending_user_monthly_statement_account.snap_shot_liquidation_fees_collected_amount = lending_user_tab_account.liquidation_fees_collected_amount;
    lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
    lending_user_monthly_statement_account.snap_shot_interest_accrued_amount = lending_user_tab_account.interest_accrued_amount;
    lending_user_monthly_statement_account.snap_shot_repaid_debt_amount = lending_user_tab_account.repaid_debt_amount;
    lending_user_monthly_statement_account.snap_shot_liquidated_amount = lending_user_tab_account.liquidated_amount;
    lending_user_monthly_statement_account.snap_shot_liquidator_amount = lending_user_tab_account.liquidator_amount;
    lending_user_monthly_statement_account.monthly_statement_account_added = true;

    msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);

    Ok(())
}

fn deposit_tokens_into_token_reserve_from_user<'info>(token_mint_address: Pubkey,
    token_reserve_ata: &InterfaceAccount<'info, TokenAccount>,
    user_ata: &InterfaceAccount<'info, TokenAccount>,
    token_mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    signer: &Signer<'info>,
    system_program_account: &Program<'info, System>,
    transfer_amount: u64
) -> Result<()>
{
    //Handle native SOL transactions
    if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
    {
        //CPI to the System Program to transfer SOL from the user to the program's wSOL ATA.
        let cpi_accounts = system_program::Transfer
        {
            from: signer.to_account_info(),
            to: token_reserve_ata.to_account_info()
        };
        let cpi_program = system_program_account.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        system_program::transfer(cpi_ctx, transfer_amount)?;

        //CPI to the SPL Token Program to "sync" the wSOL ATA's balance.
        let cpi_accounts = SyncNative
        {
            account: token_reserve_ata.to_account_info(),
        };
        let cpi_program = token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token_interface::sync_native(cpi_ctx)?;

        //Close temporary wSOL ATA if its balance is zero
        let user_balance_after_transfer = user_ata.amount;
        if user_balance_after_transfer == 0
        {
            //Since the User has no other wrapped SOL, close the temporary wrapped SOL account
            let cpi_accounts = CloseAccount
            {
                account: user_ata.to_account_info(),
                destination: signer.to_account_info(),
                authority: signer.to_account_info(),
            };
            let cpi_program = token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token_interface::close_account(cpi_ctx)?; 
        }
    }
    //Handle all other tokens
    else
    {
        //Cross Program Invocation for Token Transfer
        let cpi_accounts = TransferChecked
        {
            from: user_ata.to_account_info(),
            to: token_reserve_ata.to_account_info(),
            mint: token_mint.to_account_info(),
            authority: signer.to_account_info()
        };
        let cpi_program = token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        //Transfer Tokens Into The Reserve
        token_interface::transfer_checked(cpi_ctx, transfer_amount, token_mint.decimals)?;  
    }

    Ok(())
}

fn withdraw_tokens_from_token_reserve_to_user<'info>(token_mint_address: Pubkey,
    token_reserve: &Account<'info, TokenReserve>,
    token_reserve_ata: &InterfaceAccount<'info, TokenAccount>,
    user_ata: &InterfaceAccount<'info, TokenAccount>,
    token_mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    signer: &Signer<'info>,
    system_program_account: &Program<'info, System>,
    transfer_amount: u64
) -> Result<()>
{
    let seeds = &[b"tokenReserve", token_mint_address.as_ref(), &[token_reserve.bump]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = TransferChecked
    {
        from: token_reserve_ata.to_account_info(),
        to: user_ata.to_account_info(),
        mint: token_mint.to_account_info(),
        authority: token_reserve.to_account_info()
    };
    let cpi_program = token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

    //Transfer Tokens Back to the User
    token_interface::transfer_checked(cpi_ctx, transfer_amount, token_mint.decimals)?;

    //Handle wSOL Token unwrap
    if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
    {
        let user_balance_after_transfer = user_ata.amount;

        if user_balance_after_transfer > transfer_amount
        {
            //Since User already had wrapped SOL, only unwrapped the amount withdrawn
            let cpi_accounts = system_program::Transfer
            {
                from: user_ata.to_account_info(),
                to: signer.to_account_info()
            };
            let cpi_program = system_program_account.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            system_program::transfer(cpi_ctx, transfer_amount)?;
        }
        else
        {
            //Since the User has no other wrapped SOL, unwrap it all, send it to the User, and close the temporary wrapped SOL account
            let cpi_accounts = CloseAccount
            {
                account: user_ata.to_account_info(),
                destination: signer.to_account_info(),
                authority: signer.to_account_info(),
            };
            let cpi_program = token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token_interface::close_account(cpi_ctx)?; 
        }
    }

    Ok(())
}

#[program]
pub mod lending_protocol 
{
    use super::*;

    pub fn initialize_lending_protocol(ctx: Context<InitializeLendingProtocol>, statement_month: u8, statement_year: u16) -> Result<()> 
    {
        //Only the initial CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), INITIAL_CEO_ADDRESS, AuthorizationError::NotCEO);

        let ceo = &mut ctx.accounts.ceo;
        ceo.address = INITIAL_CEO_ADDRESS;

        let solvency_treasurer = &mut ctx.accounts.solvency_treasurer;
        solvency_treasurer.address = INITIAL_SOLVENCY_TREASURER_ADDRESS;

        let liquidation_treasurer = &mut ctx.accounts.liquidation_treasurer;
        liquidation_treasurer.address = INITIAL_LIQUIDATION_TREASURER_ADDRESS;

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

    pub fn pass_on_solvency_treasurer(ctx: Context<PassOnSolvencyTreasurer>, new_treasurer_address: Pubkey) -> Result<()> 
    {
        let solvency_treasurer = &mut ctx.accounts.solvency_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), solvency_treasurer.address.key(), AuthorizationError::NotSolvencyTreasurer);

        msg!("The Solvency Treasurer has passed on the title to a new Treasurer");
        msg!("New Treasurer: {}", new_treasurer_address.key());

        solvency_treasurer.address = new_treasurer_address.key();

        Ok(())
    }

    pub fn pass_on_liquidation_treasurer(ctx: Context<PassOnLiquidationTreasurer>, new_treasurer_address: Pubkey) -> Result<()> 
    {
        let liquidation_treasurer = &mut ctx.accounts.liquidation_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), liquidation_treasurer.address.key(), AuthorizationError::NotLiquidationTreasurer);

        msg!("The Liquidation Treasurer has passed on the title to a new Treasurer");
        msg!("New Treasurer: {}", new_treasurer_address.key());

        liquidation_treasurer.address = new_treasurer_address.key();

        Ok(())
    }

    pub fn update_current_statement_month_and_year(ctx: Context<UpdateCurrentStatementMonthAndYear>, statement_month: u8, statement_year: u16) -> Result<()> 
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
        pyth_feed_id: [u8; 32],
        fixed_borrow_apy: u16,
        use_fixed_borrow_apy: bool,
        global_limit: u128,
        solvency_insurance_fee_rate: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        //Solvency Insurance Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(solvency_insurance_fee_rate <= 10_000, InvalidInputError::InvalidSolvencyInsuranceFeeRate);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.bump = ctx.bumps.token_reserve;
        token_reserve.token_mint_address = token_mint_address.key();
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.pyth_feed_id = pyth_feed_id;
        token_reserve.borrow_apy = fixed_borrow_apy;
        token_reserve.fixed_borrow_apy = fixed_borrow_apy;
        token_reserve.use_fixed_borrow_apy = use_fixed_borrow_apy;
        token_reserve.global_limit = global_limit;
        token_reserve.solvency_insurance_fee_rate = solvency_insurance_fee_rate;
        token_reserve.supply_interest_change_index = 1_000_000_000_000_000_000;
        token_reserve.borrow_interest_change_index = 1_000_000_000_000_000_000;

        token_reserve.token_reserve_protocol_index = token_reserve_stats.token_reserve_count;
        token_reserve_stats.token_reserve_count += 1;

        let hex_string = hex::encode(pyth_feed_id);

        msg!("Added Token Reserve #{}", token_reserve_stats.token_reserve_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
        msg!("Pyth Feed ID: 0x{}", hex_string);
        msg!("Fixed Borrow APY: {}", fixed_borrow_apy);
        msg!("Use fixed Borrow APY: {}", use_fixed_borrow_apy);
        msg!("Global Limit: {}", global_limit);
            
        Ok(())
    }

    pub fn update_token_reserve(ctx: Context<UpdateTokenReserve>,
        _token_mint_address: Pubkey,
        fixed_borrow_apy: u16,
        use_fixed_borrow_apy: bool,
        global_limit: u128,
        solvency_insurance_fee_rate: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        //Solvency Insurance Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(solvency_insurance_fee_rate <= 10_000, InvalidInputError::InvalidSolvencyInsuranceFeeRate);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;

        //If the value of the Token Reserve Borrow APY will change, calculate previous interest changes before updating it
        if token_reserve.fixed_borrow_apy != fixed_borrow_apy || token_reserve.use_fixed_borrow_apy != use_fixed_borrow_apy
        {
            let time_stamp = Clock::get()?.unix_timestamp as u64;

            //Calculate Token Reserve Previously Earned And Accrued Interest
            update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;
        }

        token_reserve.fixed_borrow_apy = fixed_borrow_apy;
        token_reserve.use_fixed_borrow_apy = use_fixed_borrow_apy;
        token_reserve.global_limit = global_limit;
        token_reserve.solvency_insurance_fee_rate = solvency_insurance_fee_rate;
        token_reserve_stats.token_reserves_updated_count += 1;

        //Update Token Reserve Global Utilization Rate, Borrow APY, and, Supply APY
        update_token_reserve_rates(token_reserve)?;

        msg!("Token Reserve Updated");
        msg!("New Fixed Borrow APY: {}", fixed_borrow_apy);
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
        //SubMarket Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(fee_on_interest_earned_rate <= 10_000, InvalidInputError::InvalidSubMarketFeeRate);

        let sub_market = &mut ctx.accounts.sub_market;
        sub_market.bump = ctx.bumps.sub_market;
        sub_market.owner = ctx.accounts.signer.key();
        sub_market.fee_collector_address = fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate; //This should fed in fixed point notation from 0 to 10,000 (0 to 100%)
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
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate as f64 / 100.0); //convert from fixed point notation with 4 decimal places back to decimal for logging
        
        Ok(())
    }

    pub fn edit_sub_market(ctx: Context<EditSubMarket>,
        _token_mint_address: Pubkey,
        sub_market_index: u16,
        fee_collector_address: Pubkey,
        fee_on_interest_earned_rate: u16
    ) -> Result<()> 
    {
        //SubMarket Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(fee_on_interest_earned_rate <= 10_000, InvalidInputError::InvalidSubMarketFeeRate);

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
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate as f64 / 100.0); //convert from fixed point notation with 4 decimal places back to decimal for logging
            
        Ok(())
    }

    pub fn deposit_tokens(ctx: Context<DepositTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        account_name: Option<String> //Optional variable. Use null on front end when not needed
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        let new_token_reserve_deposited_amount = amount as u128 + token_reserve.deposited_amount;
        //You can't deposit more than the global limit
        require!(new_token_reserve_deposited_amount <= token_reserve.global_limit, InvalidInputError::GlobalLimitExceeded);

        //Populate lending user account if being newly initialized. A user can have multiple accounts based on their account index. 
        if lending_user_account.lending_user_account_added == false
        {
            let mut new_account_name_to_use: String = String::from("Generic Depositer");
            if let Some(new_account_name) = account_name
            {
                if !new_account_name.is_empty()//Check for empty string ""
                {
                    new_account_name_to_use = new_account_name;
                }
            }

            initialize_lending_user_account(
                lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use
            )?;
        }
        
        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                ctx.bumps.lending_user_tab_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed or brand new sub user account.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_monthly_statement_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;

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

        deposit_tokens_into_token_reserve_from_user(
            token_mint_address.key(),
            &ctx.accounts.token_reserve_ata,
            &ctx.accounts.user_ata,
            &ctx.accounts.mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            amount
        )?;

        //Update Values and Stat Listener
        lending_stats.deposits += 1;
        sub_market.deposited_amount += amount as u128;
        token_reserve.deposited_amount += amount as u128;
        lending_user_tab_account.deposited_amount += amount;
        lending_user_monthly_statement_account.monthly_deposited_amount += amount;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;

        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount;
        token_reserve.last_lending_activity_type = Activity::Deposit as u8;
        sub_market.last_lending_activity_amount = amount;
        sub_market.last_lending_activity_type = Activity::Deposit as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = amount;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Deposit as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;

        msg!("{} deposited at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);

        Ok(())
    }

    pub fn edit_lending_user_account_name(ctx: Context<EditLendingUserAccountName>,
        _user_account_index: u8,
        account_name: String
    ) -> Result<()> 
    {
        //Account Name string must not be longer than 25 characters
        require!(account_name.len() <= MAX_ACCOUNT_NAME_LENGTH, InvalidInputError::LendingUserAccountNameTooLong);

        let lending_user_account = &mut ctx.accounts.lending_user_account;
        lending_user_account.account_name = account_name.clone();

        let lending_user_stats = &mut ctx.accounts.lending_user_stats;
        lending_user_stats.name_change_count += 1;

        msg!("Lending User Account name updated to: {}", account_name);

        Ok(()) 
    }

    pub fn withdraw_tokens(ctx: Context<WithdrawTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        _user_account_index: u8,
        amount: u64,
        withdraw_max: bool
    ) -> Result<()> 
    {
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;

        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(token_reserve.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserve);
        require!(lending_user_account.last_health_update_clock_slot == clock_slot, LendingError::StaleLendingUser);

        //After updating interest earned and accrued, set withdraw amount
        let withdraw_amount;

        if withdraw_max
        {
            withdraw_amount = lending_user_tab_account.deposited_amount;
        }
        else
        {
            withdraw_amount = amount
        }

        //You can't withdraw more funds than you've deposited
        require!(lending_user_tab_account.deposited_amount >= withdraw_amount, LendingError::InsufficientFunds);

        //You can't withdraw or borrow more funds than are currently available in the Token Reserve. This can happen if there is too much borrowing going on.
        let available_token_amount = token_reserve.deposited_amount - token_reserve.borrowed_amount;
        require!(available_token_amount >= withdraw_amount as u128, LendingError::InsufficientLiquidity);

        if lending_user_account.total_borrowed_usd_value > 0
        {
            let price_update_account_serialized = remaining_accounts_iter.next().unwrap();
            let normalized_price_8_decimals = get_token_pyth_usd_value(price_update_account_serialized, token_reserve.pyth_feed_id)?;
            let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 
            let new_user_deposited_usd_value = lending_user_account.total_deposited_usd_value - ((withdraw_amount as u128 * normalized_price_8_decimals) / token_conversion_number);

            //Multiply before dividing to help keep precision
            let user_deposited_usd_value_x_70 = new_user_deposited_usd_value * 70;
            let seventy_percent_of_new_deposited_usd_value = user_deposited_usd_value_x_70 / 100;

            //You can't borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral.
            require!(seventy_percent_of_new_deposited_usd_value >= lending_user_account.total_borrowed_usd_value, LendingError::LiquidationExposure);
        }

        withdraw_tokens_from_token_reserve_to_user(
            token_mint_address.key(),
            token_reserve,
            &ctx.accounts.token_reserve_ata,
            &ctx.accounts.user_ata,
            &ctx.accounts.mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            withdraw_amount
        )?;
        
        //Update Values and Stat Listener
        lending_stats.withdrawals += 1;
        sub_market.deposited_amount -= withdraw_amount as u128;
        token_reserve.deposited_amount -= withdraw_amount as u128;
        lending_user_tab_account.deposited_amount -= withdraw_amount;
        lending_user_monthly_statement_account.monthly_withdrawal_amount += withdraw_amount;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
        
        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = withdraw_amount;
        token_reserve.last_lending_activity_type = Activity::Withdraw as u8;
        sub_market.last_lending_activity_amount = withdraw_amount;
        sub_market.last_lending_activity_type = Activity::Withdraw as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp; 
        lending_user_monthly_statement_account.last_lending_activity_amount = withdraw_amount;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Withdraw as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        
        msg!("{} withdrew at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);

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
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;

        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(token_reserve.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserve);
        require!(lending_user_account.last_health_update_clock_slot == clock_slot, LendingError::StaleLendingUser);

        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        //This is for when a user is borrowing a token they have never interacted with before
        if lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                ctx.bumps.lending_user_tab_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }
        //This is for when a user is borrowing a token they have never interacted with before
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_monthly_statement_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        let price_update_account_serialized = remaining_accounts_iter.next().unwrap();
        let normalized_price_8_decimals = get_token_pyth_usd_value(price_update_account_serialized, token_reserve.pyth_feed_id)?;
        let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 
        let new_user_borrowed_usd_value = lending_user_account.total_borrowed_usd_value + ((amount as u128 * normalized_price_8_decimals) / token_conversion_number);

        //Multiply before dividing to help keep precision
        let user_deposited_usd_value_x_70 = lending_user_account.total_deposited_usd_value * 70;
        let seventy_percent_of_deposited_usd_value = user_deposited_usd_value_x_70 / 100;

        //You can't borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral.
        require!(seventy_percent_of_deposited_usd_value >= new_user_borrowed_usd_value, LendingError::LiquidationExposure);

        //You can't withdraw or borrow more funds than are currently available in the Token Reserve. This can happen if there is too much borrowing going on.
        let available_token_amount = token_reserve.deposited_amount - token_reserve.borrowed_amount;
        require!(available_token_amount >= amount as u128, LendingError::InsufficientLiquidity);

        withdraw_tokens_from_token_reserve_to_user(
            token_mint_address.key(),
            token_reserve,
            &ctx.accounts.token_reserve_ata,
            &ctx.accounts.user_ata,
            &ctx.accounts.mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            amount
        )?;

        //Update Values and Stat Listener
        lending_stats.borrows += 1;
        sub_market.borrowed_amount += amount as u128;
        token_reserve.borrowed_amount += amount as u128;
        lending_user_tab_account.borrowed_amount += amount;
        lending_user_monthly_statement_account.monthly_borrowed_amount += amount;
        lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;

        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount;
        token_reserve.last_lending_activity_type = Activity::Borrow as u8;
        sub_market.last_lending_activity_amount = amount;
        sub_market.last_lending_activity_type = Activity::Borrow as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp; 
        lending_user_monthly_statement_account.last_lending_activity_amount = amount;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Borrow as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        
        msg!("{} borrowed at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);

        Ok(())
    }

    pub fn repay_tokens(ctx: Context<RepayTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        _user_account_index: u8,
        amount: u64,
        pay_off_loan: bool
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;
        
        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(token_reserve.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserve);
        require!(lending_user_account.last_health_update_clock_slot == clock_slot, LendingError::StaleLendingUser);

        //After updating interest earned and accrued(with refresh_user_health_chunk), set payment amount
        let repayment_amount;

        if pay_off_loan
        {
            repayment_amount = lending_user_tab_account.borrowed_amount;
        }
        else
        {
            repayment_amount = amount
        }

        //Multiply before dividing to help keep precision
        let user_deposited_usd_value_x_80 = lending_user_account.total_deposited_usd_value * 80;
        let eighty_percent_of_deposited_usd_value = user_deposited_usd_value_x_80 / 100;
        
        //Check if lending user account is in a liquidatable state
        if lending_user_account.total_borrowed_usd_value >= eighty_percent_of_deposited_usd_value
        {
            //Multiply before dividing to help keep precision
            let borrowed_amount_x_10 = lending_user_tab_account.borrowed_amount * 10;
            let ten_percent_of_borrowed_amount = borrowed_amount_x_10 / 100;

            //You must repay atleast 10% of the borrow position if the account is in an unhealthy state. This prevents "griefing".
            //IE: Only repaying $1 (or just the smallest enough amount to be in a healthy state), front running the liquidator so their transaction fails and holding the protocol's solvency hostage!
            require!(repayment_amount >= ten_percent_of_borrowed_amount, LendingError::GriefingRepayment);
        }

        //You can't repay an amount that is greater than your borrowed amount.
        require!(lending_user_tab_account.borrowed_amount >= repayment_amount, LendingError::TooManyFunds);

        //Repay debt
        deposit_tokens_into_token_reserve_from_user(
            token_mint_address.key(),
            &ctx.accounts.token_reserve_ata,
            &ctx.accounts.user_ata,
            &ctx.accounts.mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            repayment_amount
        )?;

        //Update Values and Stat Listener
        lending_stats.repayments += 1;
        sub_market.borrowed_amount -= repayment_amount as u128;
        sub_market.repaid_debt_amount += repayment_amount as u128;
        token_reserve.borrowed_amount -= repayment_amount as u128;
        token_reserve.repaid_debt_amount += repayment_amount as u128;
        lending_user_tab_account.borrowed_amount -= repayment_amount;
        lending_user_tab_account.repaid_debt_amount += repayment_amount;
        lending_user_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount;
        lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
        lending_user_monthly_statement_account.snap_shot_repaid_debt_amount = lending_user_tab_account.repaid_debt_amount;
        
        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = repayment_amount;
        token_reserve.last_lending_activity_type = Activity::Repay as u8;
        sub_market.last_lending_activity_amount = repayment_amount;
        sub_market.last_lending_activity_type = Activity::Repay as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
  
        msg!("{} repaid debt at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);
        
        Ok(())
    }

    pub fn liquidate_account(ctx: Context<LiquidateAccount>,
        repayment_token_mint_address: Pubkey,
        liquidation_token_mint_address: Pubkey,
        repayment_sub_market_owner_address: Pubkey,
        repayment_sub_market_index: u16,
        liquidation_sub_market_owner_address: Pubkey,
        liquidation_sub_market_index: u16,
        liquidati_account_owner_address: Pubkey,
        liquidati_account_index: u8,
        liquidator_account_index: u8,
        amount_to_repay: u64,
        repay_max: bool,
        paying_off_insolvent_account: bool,
        send_reward_to_wallet: bool,
        account_name: Option<String> //Optional variable. Use null on front end when not needed
    ) -> Result<()>
    {
        let lending_protocol = &ctx.accounts.lending_protocol;
        let repayment_token_reserve = &mut ctx.accounts.repayment_token_reserve;
        let liquidation_token_reserve = &mut ctx.accounts.liquidation_token_reserve;
        let liquidati_lending_account = &mut ctx.accounts.liquidati_lending_account;
        let liquidator_lending_account = &mut ctx.accounts.liquidator_lending_account;
        let liquidator_repayment_tab_account = &mut ctx.accounts.liquidator_repayment_tab_account;
        let liquidator_liquidation_tab_account = &mut ctx.accounts.liquidator_liquidation_tab_account;
        let liquidator_repayment_monthly_statement_account = &mut ctx.accounts.liquidator_repayment_monthly_statement_account;
        let liquidator_liquidation_monthly_statement_account = &mut ctx.accounts.liquidator_liquidation_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;

        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(repayment_token_reserve.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserve);
        require!(liquidation_token_reserve.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserve);
        require!(liquidati_lending_account.last_health_update_clock_slot == clock_slot, LendingError::StaleLendingUser);

        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        //Validate Accounts

        ///////////////
        //Lending Stats
        let lending_stats_serialized = remaining_accounts_iter.next().unwrap();
        let mut lending_stats = validate_and_return_lending_stats_account(*ctx.program_id, lending_stats_serialized)?;

        /////////////////////////////
        //Repayment SubMarket Account
        let repayment_sub_market_account_serialized = remaining_accounts_iter.next().unwrap();
        let mut repayment_sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            repayment_sub_market_account_serialized,
            repayment_token_mint_address,
            repayment_sub_market_owner_address,
            repayment_sub_market_index)?;

        ///////////////////////////////
        //Liquidation SubMarket Account
        let liquidation_sub_market_account_serialized = remaining_accounts_iter.next().unwrap();
        let mut liquidation_sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            liquidation_sub_market_account_serialized,
            liquidation_token_mint_address,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index)?;

        /////////////////////////////////
        //Liquidati Repayment Tab Account
        let liquidati_repayment_tab_account_serialized = remaining_accounts_iter.next().unwrap();
        let mut liquidati_repayment_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_repayment_tab_account_serialized,
            repayment_token_mint_address,
            repayment_sub_market_owner_address,
            repayment_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////
        //Liquidati Liquidation Tab Account
        let liquidati_liquidation_tab_account_serialized = remaining_accounts_iter.next().unwrap();
        let mut liquidati_liquidation_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_liquidation_tab_account_serialized,
            liquidation_token_mint_address,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Repayment Monthly Statement Account
        let liquidati_repayment_monthly_statement_account_serialized = remaining_accounts_iter.next().unwrap();
        let mut liquidati_repayment_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_repayment_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            repayment_token_mint_address,
            repayment_sub_market_owner_address,
            repayment_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Liquidation Monthly Statement Account
        let liquidati_liquidation_monthly_statement_account_serialized = remaining_accounts_iter.next().unwrap();
        let mut liquidati_liquidation_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_liquidation_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            liquidation_token_mint_address,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        let repayment_amount;

        //Check if Account is liquidatable and set repayment_amount
        if paying_off_insolvent_account
        {
            //You can't zero out an account whose borrow liabilities aren't 100% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= liquidati_lending_account.total_deposited_usd_value, LendingError::NotInsolvent);

            if repay_max
            {
                repayment_amount = liquidati_repayment_tab_account.borrowed_amount;
            }
            else
            {
                if amount_to_repay > liquidati_repayment_tab_account.borrowed_amount
                {
                    //Can't pay more debt than the user has accumulated
                    repayment_amount = liquidati_repayment_tab_account.borrowed_amount;
                }
                else
                {
                    repayment_amount = amount_to_repay;
                }  
            }
        }
        else
        {
            //Multiply before dividing to help keep precision
            let liquidati_deposited_usd_value_x_80 = liquidati_lending_account.total_deposited_usd_value * 80;
            let eighty_percent_of_liquidati_deposited_usd_value = liquidati_deposited_usd_value_x_80 / 100;

            //You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= eighty_percent_of_liquidati_deposited_usd_value, LendingError::NotLiquidatable);

            //Multiply before dividing to help keep precision
            let liquidati_borrowed_amount_x_50 = liquidati_repayment_tab_account.borrowed_amount * 50;
            let fifty_percent_of_liquidati_borrowed_amount = liquidati_borrowed_amount_x_50 / 100;

            if repay_max
            {
                repayment_amount = fifty_percent_of_liquidati_borrowed_amount;
            }
            else
            {
                repayment_amount = amount_to_repay;
            }

            //You can't repay more than 50% of a liquidati's debt position
            require!(repayment_amount <= fifty_percent_of_liquidati_borrowed_amount, LendingError::OverLiquidation);
        }

        //Multiply before dividing to help keep precision
        let borrowed_amount_x_10 = liquidati_repayment_tab_account.borrowed_amount * 10;
        let ten_percent_of_borrowed_amount = borrowed_amount_x_10 / 100;

        //You must repay atleast 10% of the borrow position when the account is in an unhealthy state. This prevents "griefing".
        //IE: Only repaying $1 (or just the smallest enough amount to be in a healthy state), front running other liquidators so their transaction fails and holding the protocol's solvency hostage!
        require!(repayment_amount >= ten_percent_of_borrowed_amount, LendingError::GriefingRepayment);

        //Populate lending user account if being newly initialized. A user can have multiple accounts based on their account index. 
        if liquidator_lending_account.lending_user_account_added == false
        {
            let mut new_account_name_to_use: String = String::from("Generic Liquidator");
            if let Some(new_account_name) = account_name
            {
                if !new_account_name.is_empty()
                {
                    new_account_name_to_use = new_account_name;
                }
            }

            initialize_lending_user_account(
                liquidator_lending_account,
                ctx.accounts.signer.key(),
                liquidator_account_index,
                new_account_name_to_use
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if liquidator_repayment_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_repayment_tab_account,
                ctx.bumps.liquidator_repayment_tab_account,
                repayment_token_mint_address.key(),
                repayment_sub_market_owner_address.key(),
                repayment_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index
            )?;
        }
        if liquidator_liquidation_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_liquidation_tab_account,
                ctx.bumps.liquidator_liquidation_tab_account,
                liquidation_token_mint_address.key(),
                liquidation_sub_market_owner_address.key(),
                liquidation_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed or brand new sub user account.
        if liquidator_repayment_monthly_statement_account.monthly_statement_account_added == false
        {
            initialize_lending_user_monthly_statement_account(
                liquidator_repayment_monthly_statement_account,
                liquidator_repayment_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_repayment_monthly_statement_account,
                repayment_token_mint_address.key(),
                repayment_sub_market_owner_address,
                repayment_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index,
            )?;
        }
        if liquidator_liquidation_monthly_statement_account.monthly_statement_account_added == false
        {
            initialize_lending_user_monthly_statement_account(
                liquidator_liquidation_monthly_statement_account,
                liquidator_liquidation_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_liquidation_monthly_statement_account,
                liquidation_token_mint_address.key(),
                liquidation_sub_market_owner_address,
                liquidation_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index,
            )?;
        }

        //Update interest earned and accrued for the liquidator
        //This is done incase the liquidator decides to deposit their rewards to the protocol, in their liquidation tab account in this case
        update_user_previous_interest_earned(
            liquidation_token_reserve,
            &mut liquidation_sub_market,
            liquidator_liquidation_tab_account,
            liquidator_liquidation_monthly_statement_account
        )?;
        update_user_previous_interest_accrued(
            liquidation_token_reserve,
            &mut liquidation_sub_market,
            liquidator_liquidation_tab_account,
            liquidator_liquidation_monthly_statement_account
        )?;

        //Repay Liquidati's Debt
        deposit_tokens_into_token_reserve_from_user(
            repayment_token_mint_address.key(),
            &ctx.accounts.repayment_token_reserve_ata,
            &ctx.accounts.liquidator_repayment_ata,
            &ctx.accounts.repayment_mint,
            &ctx.accounts.repayment_token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            repayment_amount
        )?;

        //Get USD value of Repayment Amount
        let repayment_price_update_account_serialized = remaining_accounts_iter.next().unwrap();
        let repayment_token_conversion_number = BASE_10_INT.pow(repayment_token_reserve.token_decimal_amount as u32); 
        let repayment_token_usd_value = get_token_pyth_usd_value(repayment_price_update_account_serialized, repayment_token_reserve.pyth_feed_id)?;
        let repayment_amount_usd_value = (repayment_amount as u128 * repayment_token_usd_value) / repayment_token_conversion_number;

        //Get USD value of Liquidation Token
        let liquidation_price_update_account_serialized = remaining_accounts_iter.next().unwrap();
        let liquidation_token_conversion_number = BASE_10_INT.pow(liquidation_token_reserve.token_decimal_amount as u32); 
        let liquidation_token_usd_value = get_token_pyth_usd_value(liquidation_price_update_account_serialized, liquidation_token_reserve.pyth_feed_id)?;

        let amount_to_be_liquidated = ((repayment_amount_usd_value * liquidation_token_conversion_number) / liquidation_token_usd_value) as u64;

        //Liquidate part of the Liquidati's Collateral and Transfer it plus a 7% bonus to the Liquidator
        //Multiply before dividing to help keep precision
        let amount_to_be_liquidated_x_107 = amount_to_be_liquidated * 107;
        let mut liquidation_amount_with_7_percent_bonus = amount_to_be_liquidated_x_107 / 100;

        //Take a 1% liquidation fee
        let mut liquidation_fee_amount = amount_to_be_liquidated / 100;

        //Check for underflow
        if liquidati_liquidation_tab_account.deposited_amount < liquidation_amount_with_7_percent_bonus + liquidation_fee_amount
        {
            //Take a 1% liquidation fee
            liquidation_fee_amount = liquidati_liquidation_tab_account.deposited_amount / 100;
            //Give remainder to liquidator
            liquidation_amount_with_7_percent_bonus = liquidati_liquidation_tab_account.deposited_amount - liquidation_fee_amount;
        }

        //Update Repayment Values
        repayment_sub_market.borrowed_amount -= repayment_amount as u128;
        repayment_sub_market.repaid_debt_amount += repayment_amount as u128;
        repayment_token_reserve.borrowed_amount -= repayment_amount as u128;
        repayment_token_reserve.repaid_debt_amount += repayment_amount as u128;
        liquidati_repayment_tab_account.borrowed_amount -= repayment_amount;
        liquidator_repayment_tab_account.repaid_debt_amount += repayment_amount;
        liquidator_repayment_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount;
        liquidati_repayment_monthly_statement_account.snap_shot_debt_amount = liquidati_repayment_tab_account.borrowed_amount;
        liquidator_repayment_monthly_statement_account.snap_shot_repaid_debt_amount = liquidator_repayment_tab_account.repaid_debt_amount;

        //Update Liquidation and Fee Values
        liquidation_sub_market.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        liquidation_sub_market.liquidated_amount += liquidation_fee_amount as u128;
        liquidation_sub_market.deposited_amount -= liquidation_fee_amount as u128;
        liquidation_sub_market.liquidation_fees_generated_amount += liquidation_fee_amount as u128;
        liquidation_token_reserve.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        liquidation_token_reserve.liquidated_amount += liquidation_fee_amount as u128;
        liquidation_token_reserve.deposited_amount -= liquidation_fee_amount as u128;
        liquidation_token_reserve.liquidation_fees_generated_amount += liquidation_fee_amount as u128;
        liquidation_token_reserve.uncollected_liquidation_fees_amount += liquidation_fee_amount as u128;
        liquidati_liquidation_tab_account.deposited_amount -= liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_tab_account.deposited_amount -= liquidation_fee_amount;
        liquidati_liquidation_tab_account.liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_tab_account.liquidated_amount += liquidation_fee_amount;
        liquidator_liquidation_tab_account.liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_liquidation_tab_account.liquidation_fees_generated_amount += liquidation_fee_amount;
        liquidati_liquidation_monthly_statement_account.monthly_liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_monthly_statement_account.monthly_liquidated_amount += liquidation_fee_amount;
        liquidati_liquidation_monthly_statement_account.snap_shot_liquidated_amount = liquidati_liquidation_tab_account.liquidated_amount;
        liquidati_liquidation_monthly_statement_account.snap_shot_balance_amount = liquidati_liquidation_tab_account.deposited_amount;
        liquidator_liquidation_monthly_statement_account.monthly_liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_liquidation_monthly_statement_account.monthly_liquidation_fees_generated_amount += liquidation_fee_amount;
        liquidator_liquidation_monthly_statement_account.snap_shot_liquidator_amount = liquidator_liquidation_tab_account.liquidator_amount;
        liquidator_liquidation_monthly_statement_account.snap_shot_liquidation_fees_generated_amount = liquidator_liquidation_tab_account.liquidation_fees_generated_amount;
        
        if send_reward_to_wallet
        {
            withdraw_tokens_from_token_reserve_to_user(
                liquidation_token_mint_address.key(),
                liquidation_token_reserve,
                &ctx.accounts.liquidation_token_reserve_ata,
                &ctx.accounts.liquidator_liquidation_ata,
                &ctx.accounts.liquidation_mint,
                &ctx.accounts.liquidation_token_program,
                &ctx.accounts.signer,
                &ctx.accounts.system_program,
                liquidation_amount_with_7_percent_bonus
            )?;

            liquidation_sub_market.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128;
            liquidation_token_reserve.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128;
        }
        else
        {
            liquidator_liquidation_tab_account.deposited_amount += liquidation_amount_with_7_percent_bonus;
            liquidator_liquidation_monthly_statement_account.snap_shot_balance_amount = liquidator_liquidation_tab_account.deposited_amount;
        }
        
        //Update Stat Listener
        lending_stats.liquidations += 1;
        
        //Update Repayment Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(repayment_token_reserve)?;
        repayment_sub_market.supply_interest_change_index = repayment_token_reserve.supply_interest_change_index;
        repayment_sub_market.borrow_interest_change_index = repayment_token_reserve.borrow_interest_change_index;
        liquidati_repayment_tab_account.supply_interest_change_index = repayment_token_reserve.supply_interest_change_index;
        liquidati_repayment_tab_account.borrow_interest_change_index = repayment_token_reserve.borrow_interest_change_index;

        //Update Liquidation Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(liquidation_token_reserve)?;
        liquidation_sub_market.supply_interest_change_index = liquidation_token_reserve.supply_interest_change_index;
        liquidation_sub_market.borrow_interest_change_index = liquidation_token_reserve.borrow_interest_change_index;
        liquidati_liquidation_tab_account.supply_interest_change_index = liquidation_token_reserve.supply_interest_change_index;
        liquidati_liquidation_tab_account.borrow_interest_change_index = liquidation_token_reserve.borrow_interest_change_index;
        liquidator_liquidation_tab_account.supply_interest_change_index = liquidation_token_reserve.supply_interest_change_index;
        liquidator_liquidation_tab_account.borrow_interest_change_index = liquidation_token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        repayment_token_reserve.last_lending_activity_amount = repayment_amount;
        repayment_token_reserve.last_lending_activity_type = Activity::Repay as u8;
        liquidation_token_reserve.last_lending_activity_amount = repayment_amount;
        liquidation_token_reserve.last_lending_activity_type = Activity::Liquidate as u8;
        repayment_sub_market.last_lending_activity_amount = repayment_amount;
        repayment_sub_market.last_lending_activity_type = Activity::Repay as u8;
        repayment_sub_market.last_lending_activity_time_stamp = repayment_token_reserve.last_lending_activity_time_stamp;
        liquidation_sub_market.last_lending_activity_amount = repayment_amount;
        liquidation_sub_market.last_lending_activity_type = Activity::Liquidate as u8;
        liquidation_sub_market.last_lending_activity_time_stamp = liquidation_token_reserve.last_lending_activity_time_stamp;
        liquidati_repayment_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidati_repayment_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        liquidati_repayment_monthly_statement_account.last_lending_activity_time_stamp = repayment_token_reserve.last_lending_activity_time_stamp;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_time_stamp = liquidation_token_reserve.last_lending_activity_time_stamp;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_time_stamp = liquidation_token_reserve.last_lending_activity_time_stamp;
        
        msg!("{} liquidated {}", ctx.accounts.signer.key(), liquidati_account_owner_address.key());

        msg!("Repaid debt at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        repayment_token_mint_address.key(),
        repayment_sub_market_owner_address.key(),
        repayment_sub_market_index);

        msg!("Liquidated collateral at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        liquidation_token_mint_address.key(),
        liquidation_sub_market_owner_address.key(),
        liquidation_sub_market_index);

        Ok(())
    }

    //You have to call this instruction for all user tab accounts before calling the withdraw, borrow, or liquidate functions in the same transaction.
    //It's recommended to call this refresh function on up to 4 tab sets only at a time.
    //1 set of accounts in remaining accounts for refresh (in this order) Example: LendingUserTabAccount, TokenReserve, Submarket, LendingUserMonthlyStatementAccount, and PriceUpdateV2
    pub fn refresh_user_health_chunk(ctx: Context<RefreshUserHealthChunk>, user_account_owner_address: Pubkey, user_account_index: u8) -> Result<()> 
    {
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        let lending_protocol = &ctx.accounts.lending_protocol;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;
        let clock_slot = Clock::get()?.slot;

        //If this is a new slot, reset the accumulator
        if lending_user_account.last_health_update_clock_slot != clock_slot
        {
            lending_user_account.temp_deposit_usd_value = 0;
            lending_user_account.temp_borrow_usd_value = 0;
            lending_user_account.next_tab_index_to_refresh = 0;
        }

        while let Some(tab_account_serialized) = remaining_accounts_iter.next()
        {
            //Validate Accounts
            
            /////////////
            //Tab Account
            let data_ref = tab_account_serialized.data.borrow();
            let mut data_slice: &[u8] = data_ref.deref();

            let unvalidated_lending_user_tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

            let mut lending_user_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
                tab_account_serialized,
                unvalidated_lending_user_tab_account.token_mint_address,
                unvalidated_lending_user_tab_account.sub_market_owner_address,
                unvalidated_lending_user_tab_account.sub_market_index,
                user_account_owner_address,
                user_account_index)?;

            //You must provide all of the sub user's tab accounts ordered by user_tab_account_index
            require!(lending_user_account.next_tab_index_to_refresh == lending_user_tab_account.user_tab_account_index, InvalidInputError::IncorrectOrderOfTabAccounts);

            ///////////////////////
            //Token Reserve Account
            let token_reserve_account_serialized = remaining_accounts_iter.next().unwrap();
            let mut token_reserve = validate_and_return_token_reserve_account(*ctx.program_id,
                token_reserve_account_serialized,
                lending_user_tab_account.token_mint_address)?;

            ///////////////////
            //SubMarket Account
            let sub_market_account_serialized = remaining_accounts_iter.next().unwrap();
            let mut sub_market = validate_and_return_sub_market_account(*ctx.program_id,
                sub_market_account_serialized,
                lending_user_tab_account.token_mint_address,
                lending_user_tab_account.sub_market_owner_address,
                lending_user_tab_account.sub_market_index)?;

            ///////////////////////////
            //Monthly Statement Account
            let monthly_statement_account_serialized = remaining_accounts_iter.next().unwrap();
            let mut monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
                monthly_statement_account_serialized,
                lending_protocol.current_statement_month,
                lending_protocol.current_statement_year,
                lending_user_tab_account.token_mint_address,
                lending_user_tab_account.sub_market_owner_address,
                lending_user_tab_account.sub_market_index,
                user_account_owner_address,
                user_account_index)?;

            ///////////////////////////
            //Pyth Price Update Account
            let price_update_account_serialized = remaining_accounts_iter.next().unwrap();
            let normalized_price_8_decimals = get_token_pyth_usd_value(price_update_account_serialized, token_reserve.pyth_feed_id)?;
            //msg!("Normalized Price with 8 Decimals: {}", normalized_price_8_decimals);

            //Calculate Token Reserve Previously Earned And Accrued Interest
            if token_reserve.last_health_update_clock_slot != clock_slot
            {
                update_token_reserve_supply_and_borrow_interest_change_index(&mut token_reserve, time_stamp, Some(clock_slot))?;
            }
            
            update_user_previous_interest_earned(
                &mut token_reserve,
                &mut sub_market,
                &mut lending_user_tab_account,
                &mut monthly_statement_account
            )?;

            update_user_previous_interest_accrued(
                &mut token_reserve,
                &mut sub_market,
                &mut lending_user_tab_account,
                &mut monthly_statement_account
            )?;
            
            //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
            update_token_reserve_rates(&mut token_reserve)?;
            sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
            sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
            lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
            lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

            //Update temp deposited and borrow values
            let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 
            lending_user_account.temp_deposit_usd_value += (lending_user_tab_account.deposited_amount as u128 * normalized_price_8_decimals) / token_conversion_number;
            lending_user_account.temp_borrow_usd_value += (lending_user_tab_account.borrowed_amount as u128 * normalized_price_8_decimals) / token_conversion_number;

            lending_user_account.next_tab_index_to_refresh += 1;
        }

        //Finalize if we've covered all of the Lending User's Tab Accounts
        if lending_user_account.next_tab_index_to_refresh == lending_user_account.tab_account_count
        {
            lending_user_account.total_deposited_usd_value = lending_user_account.temp_deposit_usd_value;
            lending_user_account.total_borrowed_usd_value = lending_user_account.temp_borrow_usd_value;
            lending_user_account.last_health_update_clock_slot = clock_slot;

            msg!("{} updated the health factor for Account Address: {}, Account Index: {}",
            ctx.accounts.signer.key(),
            user_account_owner_address.key(),
            user_account_index);
        }

        Ok(())
    }

    
    pub fn create_new_monthly_statement(ctx: Context<CreateNewMonthlyStatement>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_owner_address: Pubkey,
        user_account_index: u8
    ) -> Result<()> 
    {
        let lending_protocol = &ctx.accounts.lending_protocol;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;

        initialize_lending_user_monthly_statement_account(
            lending_user_monthly_statement_account,
            lending_user_tab_account,
            lending_protocol,
            ctx.bumps.lending_user_monthly_statement_account,
            token_mint_address.key(),
            sub_market_owner_address.key(),
            sub_market_index,
            user_account_owner_address.key(),
            user_account_index,
        )?;

        Ok(())
    }


    pub fn claim_sub_market_fees(ctx: Context<ClaimSubMarketFees>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String> //Optional variable. Use null on front end when not needed
    ) -> Result<()> 
    {
        let sub_market = &mut ctx.accounts.sub_market;
        //Only the Fee Collector can call this function
        require_keys_eq!(ctx.accounts.signer.key(), sub_market.fee_collector_address.key(), AuthorizationError::NotFeeCollector);

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Populate lending user account if being newly initialized. A user can have multiple accounts based on their account index. 
        if lending_user_account.lending_user_account_added == false
        {
            let mut new_account_name_to_use: String = String::from("Generic Sub Fee Claimer");
            if let Some(new_account_name) = account_name
            {
                if !new_account_name.is_empty()
                {
                    new_account_name_to_use = new_account_name;
                }
            }

            initialize_lending_user_account(
                lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                ctx.bumps.lending_user_tab_account,
                token_mint_address.key(), 
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_monthly_statement_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;

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
        token_reserve.deposited_amount += sub_market.uncollected_sub_market_fees_amount;
        sub_market.deposited_amount += sub_market.uncollected_sub_market_fees_amount;
        lending_user_tab_account.deposited_amount += sub_market.uncollected_sub_market_fees_amount as u64;
        lending_user_tab_account.sub_market_fees_collected_amount += sub_market.uncollected_sub_market_fees_amount as u64;
        lending_user_monthly_statement_account.monthly_sub_market_fees_collected_amount += sub_market.uncollected_sub_market_fees_amount as u64;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
        lending_user_monthly_statement_account.snap_shot_sub_market_fees_collected_amount = lending_user_tab_account.sub_market_fees_collected_amount;

        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Stat Listener
        lending_stats.fee_collections += 1;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = sub_market.uncollected_sub_market_fees_amount as u64;
        token_reserve.last_lending_activity_type = Activity::CollectSubMarketFees as u8;
        sub_market.last_lending_activity_amount = sub_market.uncollected_sub_market_fees_amount as u64;
        sub_market.last_lending_activity_type = Activity::CollectSubMarketFees as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = sub_market.uncollected_sub_market_fees_amount as u64;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::CollectSubMarketFees as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;

        sub_market.uncollected_sub_market_fees_amount = 0;

        msg!("{} Collected SubMarket Fees at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

        Ok(())
    }

    pub fn claim_sub_market_fees_and_deposit_in_different_sub_market(ctx: Context<ClaimSubMarketFeesAndDepositInDifferentSubMarket>,
        token_mint_address: Pubkey,
        initial_sub_market_owner_address: Pubkey,
        initial_sub_market_index: u16,
        destination_sub_market_owner_address: Pubkey,
        destination_sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String> //Optional variable. Use null on front end when not needed
    ) -> Result<()> 
    {
        let initial_sub_market = &mut ctx.accounts.initial_sub_market;
        //Only the Fee Collector can call this function
        require_keys_eq!(ctx.accounts.signer.key(), initial_sub_market.fee_collector_address.key(), AuthorizationError::NotFeeCollector);
                
        //Duplicate SubMarket Detected
        //When accounts are the exact same, it can lead to unexpected behavior where only one of them gets updated and would require extra steps
        require!(initial_sub_market_owner_address.key() != destination_sub_market_owner_address.key() ||
        initial_sub_market_index != destination_sub_market_index, LendingError::DuplicateSubMarket);

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let destination_sub_market = &mut ctx.accounts.destination_sub_market;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let initial_lending_user_tab_account = &mut ctx.accounts.initial_lending_user_tab_account;
        let destination_lending_user_tab_account = &mut ctx.accounts.destination_lending_user_tab_account;
        let initial_lending_user_monthly_statement_account = &mut ctx.accounts.initial_lending_user_monthly_statement_account;
        let destination_lending_user_monthly_statement_account = &mut ctx.accounts.destination_lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Populate lending user account if being newly initialized. A user can have multiple accounts based on their account index. 
        if lending_user_account.lending_user_account_added == false
        {
            let mut new_account_name_to_use: String = String::from("Generic Sub Fee Claimer");
            if let Some(new_account_name) = account_name
            {
                if !new_account_name.is_empty()
                {
                    new_account_name_to_use = new_account_name;
                }
            }

            initialize_lending_user_account(
                lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if initial_lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                initial_lending_user_tab_account,
                ctx.bumps.initial_lending_user_tab_account,
                token_mint_address.key(),
                initial_sub_market_owner_address,
                initial_sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }
        if destination_lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                destination_lending_user_tab_account,
                ctx.bumps.destination_lending_user_tab_account,
                token_mint_address.key(),
                destination_sub_market_owner_address,
                destination_sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed.
        if initial_lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                initial_lending_user_monthly_statement_account,
                initial_lending_user_tab_account,
                lending_protocol,
                ctx.bumps.initial_lending_user_monthly_statement_account,
                token_mint_address.key(),
                initial_sub_market_owner_address,
                initial_sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }
        if destination_lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                destination_lending_user_monthly_statement_account,
                destination_lending_user_tab_account,
                lending_protocol,
                ctx.bumps.destination_lending_user_monthly_statement_account,
                token_mint_address.key(),
                destination_sub_market_owner_address,
                destination_sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;

        update_user_previous_interest_earned(
            token_reserve,
            destination_sub_market,
            destination_lending_user_tab_account,
            destination_lending_user_monthly_statement_account
        )?;

        update_user_previous_interest_accrued(
            token_reserve,
            destination_sub_market,
            destination_lending_user_tab_account,
            destination_lending_user_monthly_statement_account
        )?;

        //Collect Fees
        token_reserve.deposited_amount += initial_sub_market.uncollected_sub_market_fees_amount;
        destination_sub_market.deposited_amount += initial_sub_market.uncollected_sub_market_fees_amount;
        destination_lending_user_tab_account.deposited_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64;
        initial_lending_user_tab_account.sub_market_fees_collected_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64;
        initial_lending_user_monthly_statement_account.monthly_sub_market_fees_collected_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64;
        initial_lending_user_monthly_statement_account.monthly_withdrawal_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64; //Treating this as a withdrawal from initial submarket. The fee collection and withdrawal cancel each other out, so no need to update snap shot balance for initial submarket.
        initial_lending_user_monthly_statement_account.snap_shot_sub_market_fees_collected_amount = initial_lending_user_tab_account.sub_market_fees_collected_amount;
        destination_lending_user_monthly_statement_account.monthly_deposited_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64; //Treating this as a deposit into destination submarket.
        destination_lending_user_monthly_statement_account.snap_shot_balance_amount = destination_lending_user_tab_account.deposited_amount;

        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        destination_sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        destination_sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        destination_lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        destination_lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Stat Listener
        lending_stats.fee_collections += 1;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = initial_sub_market.uncollected_sub_market_fees_amount as u64;
        token_reserve.last_lending_activity_type = Activity::CollectSubMarketFees as u8;
        initial_sub_market.last_lending_activity_amount = initial_sub_market.uncollected_sub_market_fees_amount as u64;
        initial_sub_market.last_lending_activity_type = Activity::CollectSubMarketFees as u8;
        initial_sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        destination_sub_market.last_lending_activity_amount = initial_sub_market.uncollected_sub_market_fees_amount as u64;
        destination_sub_market.last_lending_activity_type = Activity::Deposit as u8;
        destination_sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        initial_lending_user_monthly_statement_account.last_lending_activity_amount = initial_sub_market.uncollected_sub_market_fees_amount as u64;
        initial_lending_user_monthly_statement_account.last_lending_activity_type = Activity::CollectSubMarketFees as u8;
        initial_lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        destination_lending_user_monthly_statement_account.last_lending_activity_amount = initial_sub_market.uncollected_sub_market_fees_amount as u64;
        destination_lending_user_monthly_statement_account.last_lending_activity_type = Activity::Deposit as u8;
        destination_lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;

        initial_sub_market.uncollected_sub_market_fees_amount = 0;

        msg!("{} Collected SubMarket Fees at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        initial_sub_market_owner_address.key(),
        initial_sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

        msg!("Fees Moved to DestinationSubMarketOwner: {}, DestinationSubMarketIndex: {}", destination_sub_market_owner_address.key(), destination_sub_market_index);

        Ok(())
    }

    pub fn claim_solvency_insurance_fees(ctx: Context<ClaimSolvencyInsuranceFees>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String> //Optional variable. Use null on front end when not needed
    ) -> Result<()> 
    {
        let solvency_treasurer = &ctx.accounts.solvency_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), solvency_treasurer.address.key(), AuthorizationError::NotSolvencyTreasurer);

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Populate lending user account if being newly initialized. A user can have multiple accounts based on their account index. 
        if lending_user_account.lending_user_account_added == false
        {
            let mut new_account_name_to_use: String = String::from("Generic Ins Fee Claimer");
            if let Some(new_account_name) = account_name
            {
                if !new_account_name.is_empty()
                {
                    new_account_name_to_use = new_account_name;
                }
            }

            initialize_lending_user_account(
                lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                ctx.bumps.lending_user_tab_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_monthly_statement_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        let amount = token_reserve.uncollected_solvency_insurance_fees_amount as u64;
        withdraw_tokens_from_token_reserve_to_user(
            token_mint_address.key(),
            token_reserve,
            &ctx.accounts.token_reserve_ata,
            &ctx.accounts.treasurer_ata,
            &ctx.accounts.mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            amount
        )?;

        //Record Solvency Insurance Fee Collection
        lending_user_tab_account.solvency_insurance_fees_collected_amount += amount;
        lending_user_monthly_statement_account.monthly_solvency_insurance_fees_collected_amount += amount;
        lending_user_monthly_statement_account.snap_shot_solvency_insurance_fees_collected_amount = lending_user_tab_account.solvency_insurance_fees_collected_amount;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = token_reserve.uncollected_solvency_insurance_fees_amount as u64;
        token_reserve.last_lending_activity_type = Activity::CollectSolvencyFees as u8;
        lending_user_monthly_statement_account.last_lending_activity_amount = token_reserve.uncollected_solvency_insurance_fees_amount as u64;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::CollectSolvencyFees as u8;
        //No interest change calculated since the solvency fees are sent to the Solvency Wallet outside the protocol, so not using the time stamp on the Token Reserve
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;

        token_reserve.uncollected_solvency_insurance_fees_amount = 0;

        //Stat Listener
        lending_stats.fee_collections += 1;

        msg!("{} Collected Solvency Insurance Fees at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

        Ok(())
    }

    pub fn claim_liquidation_fees(ctx: Context<ClaimLiquidationFees>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String> //Optional variable. Use null on front end when not needed
    ) -> Result<()> 
    {
        let liquidation_treasurer = &ctx.accounts.liquidation_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), liquidation_treasurer.address.key(), AuthorizationError::NotLiquidationTreasurer);

        let lending_stats = &mut ctx.accounts.lending_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;

        //Populate lending user account if being newly initialized. A user can have multiple accounts based on their account index. 
        if lending_user_account.lending_user_account_added == false
        {
            let mut new_account_name_to_use: String = String::from("Generic Liq Fee Claimer");
            if let Some(new_account_name) = account_name
            {
                if !new_account_name.is_empty()
                {
                    new_account_name_to_use = new_account_name;
                }
            }

            initialize_lending_user_account(
                lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user enteracts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                ctx.bumps.lending_user_tab_account,
                token_mint_address.key(), 
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_monthly_statement_account,
                token_mint_address.key(),
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        //Calculate Token Reserve Previously Earned And Accrued Interest
        update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;

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
        token_reserve.deposited_amount += token_reserve.uncollected_liquidation_fees_amount;
        sub_market.deposited_amount += token_reserve.uncollected_liquidation_fees_amount;
        lending_user_tab_account.deposited_amount += token_reserve.uncollected_liquidation_fees_amount as u64;
        lending_user_tab_account.liquidation_fees_collected_amount += token_reserve.uncollected_liquidation_fees_amount as u64;
        lending_user_monthly_statement_account.monthly_sub_market_fees_collected_amount += token_reserve.uncollected_liquidation_fees_amount as u64;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
        lending_user_monthly_statement_account.snap_shot_liquidation_fees_generated_amount = lending_user_tab_account.liquidation_fees_collected_amount;

        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Stat Listener
        lending_stats.fee_collections += 1;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = token_reserve.uncollected_liquidation_fees_amount as u64;
        token_reserve.last_lending_activity_type = Activity::CollectLiquidationFees as u8;
        sub_market.last_lending_activity_amount = token_reserve.uncollected_liquidation_fees_amount as u64;
        sub_market.last_lending_activity_type = Activity::CollectLiquidationFees as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = token_reserve.uncollected_liquidation_fees_amount as u64;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::CollectLiquidationFees as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;

        token_reserve.uncollected_liquidation_fees_amount = 0;

        msg!("{} Collected Liquidation Fees at token mint address: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_mint_address.key(),
        sub_market_owner_address.key(),
        sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

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
        seeds = [b"solvencyTreasurer".as_ref()],
        bump,
        space = size_of::<SolvencyTreasurer>() + 8)]
    pub solvency_treasurer: Account<'info, SolvencyTreasurer>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"liquidationTreasurer".as_ref()],
        bump,
        space = size_of::<LiquidationTreasurer>() + 8)]
    pub liquidation_treasurer: Account<'info, LiquidationTreasurer>,

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
pub struct PassOnSolvencyTreasurer<'info> 
{
    #[account(
        mut,
        seeds = [b"solvencyTreasurer".as_ref()],
        bump)]
    pub solvency_treasurer: Account<'info, SolvencyTreasurer>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct PassOnLiquidationTreasurer<'info> 
{
    #[account(
        mut,
        seeds = [b"liquidationTreasurer".as_ref()],
        bump)]
    pub liquidation_treasurer: Account<'info, LiquidationTreasurer>,

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
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
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
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, LendingStats>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

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
    pub lending_user_account: Account<'info, LendingUserAccount>,

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
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be deposited as wSol and the user may or may not have a wSol account already.
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
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
    pub lending_user_account: Account<'info, LendingUserAccount>,

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
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

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
        mut,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be withdrawn as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
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
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

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
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //Init ATA account of token being borrowed if it doesn't exist for User
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
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
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

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
        mut,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSol and the user may or may not have a wSol account already.
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(repayment_token_mint_address: Pubkey,
    liquidation_token_mint_address: Pubkey,
    repayment_sub_market_owner_address: Pubkey,
    repayment_sub_market_index: u16,
    liquidation_sub_market_owner_address: Pubkey,
    liquidation_sub_market_index: u16,
    liquidati_account_owner_address: Pubkey,
    liquidati_account_index: u8,
    liquidator_account_index: u8)]
pub struct LiquidateAccount<'info>
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), repayment_token_mint_address.key().as_ref()], 
        bump)]
    pub repayment_token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), liquidation_token_mint_address.key().as_ref()], 
        bump)]
    pub liquidation_token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), liquidati_account_owner_address.key().as_ref(), liquidati_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub liquidati_lending_account: Box<Account<'info, LendingUserAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), liquidator_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub liquidator_lending_account: Box<Account<'info, LendingUserAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserTabAccount".as_ref(),
        repayment_token_mint_address.key().as_ref(),
        repayment_sub_market_owner_address.key().as_ref(),
        repayment_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidati_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub liquidator_repayment_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserTabAccount".as_ref(),
        liquidation_token_mint_address.key().as_ref(),
        liquidation_sub_market_owner_address.key().as_ref(),
        liquidation_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidator_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub liquidator_liquidation_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        repayment_token_mint_address.key().as_ref(),
        repayment_sub_market_owner_address.key().as_ref(),
        repayment_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidator_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_repayment_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        liquidation_token_mint_address.key().as_ref(),
        liquidation_sub_market_owner_address.key().as_ref(),
        liquidation_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidator_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_liquidation_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = repayment_mint,
        associated_token::authority = signer,
        associated_token::token_program = repayment_token_program
    )]
    pub liquidator_repayment_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = liquidation_mint,
        associated_token::authority = signer,
        associated_token::token_program = liquidation_token_program
    )]
    pub liquidator_liquidation_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = repayment_mint,
        associated_token::authority = repayment_token_reserve,
        associated_token::token_program = repayment_token_program
    )]
    pub repayment_token_reserve_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = liquidation_mint,
        associated_token::authority = liquidation_token_reserve,
        associated_token::token_program = liquidation_token_program
    )]
    pub liquidation_token_reserve_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    pub repayment_mint: Box<InterfaceAccount<'info, Mint>>,
    pub liquidation_mint: Box<InterfaceAccount<'info, Mint>>,
    pub repayment_token_program: Interface<'info, TokenInterface>,
    pub liquidation_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

//The monthly statement accounts have to exists before calling the refresh_user_health_chunk instruction.
//Use the create_new_monthly_statement function if it's a new month and it doesn't exist yet.
#[derive(Accounts)]
#[instruction(user_account_owner_address: Pubkey, user_account_index: u8)]
pub struct RefreshUserHealthChunk<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), user_account_owner_address.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner_address: Pubkey,
    user_account_index: u8)]
pub struct CreateNewMonthlyStatement<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        user_account_owner_address.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        user_account_owner_address.key().as_ref(),
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
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), sub_market_owner_address.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, SubMarket>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

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

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey,
    initial_sub_market_owner_address: Pubkey,
    initial_sub_market_index: u16,
    destination_sub_market_owner_address: Pubkey,
    destination_sub_market_index: u16,
    user_account_index: u8)]
pub struct ClaimSubMarketFeesAndDepositInDifferentSubMarket<'info> 
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
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), initial_sub_market_owner_address.key().as_ref(), initial_sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub initial_sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_mint_address.key().as_ref(), destination_sub_market_owner_address.key().as_ref(), destination_sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub destination_sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        initial_sub_market_owner_address.key().as_ref(),
        initial_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub initial_lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_mint_address.key().as_ref(),
        destination_sub_market_owner_address.key().as_ref(),
        destination_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub destination_lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        initial_sub_market_owner_address.key().as_ref(),
        initial_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub initial_lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        destination_sub_market_owner_address.key().as_ref(),
        destination_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub destination_lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct ClaimSolvencyInsuranceFees<'info> 
{
    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, LendingStats>>,

    #[account(
        seeds = [b"solvencyTreasurer".as_ref()],
        bump)]
    pub solvency_treasurer: Account<'info, SolvencyTreasurer>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

    //The SubMarket doesn't matter that much here since all of the fees are collected from the Token Reserve, but a SubMarket is still neccessary for using the tab account
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

    //The SubMarket doesn't matter that much here since all of the fees are collected from the Token Reserve, but a SubMarket is still neccessary for using the monthly statements
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
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be claimed as wSOL then converted to SOL for Treasurer. This function also closes wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub treasurer_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u16, user_account_index: u8)]
pub struct ClaimLiquidationFees<'info> 
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
        seeds = [b"liquidationTreasurer".as_ref()],
        bump)]
    pub liquidation_treasurer: Account<'info, LiquidationTreasurer>,

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
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

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
pub struct LendingProtocol
{
    pub current_statement_month: u8,
    pub current_statement_year: u16
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
    pub token_reserve_protocol_index: u32,
    pub token_mint_address: Pubkey,
    pub token_decimal_amount: u8,
    pub pyth_feed_id: [u8; 32],
    pub supply_apy: u16,
    pub borrow_apy: u16,
    pub fixed_borrow_apy: u16,
    pub use_fixed_borrow_apy: bool,
    pub utilization_rate: u16,
    pub global_limit: u128,
    pub supply_interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Supply User interest is earned from Borrow Users so that it can be proportionally distributed to Supply Users
    pub borrow_interest_change_index: u128, //Starts at 1 (in fixed point notation) and increases as Borrow User interest is accrued for Supply Users so that it can be proportionally distributed to Borrow Users
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub sub_market_fees_generated_amount: u128,
    pub solvency_insurance_fee_rate: u16,
    pub solvency_insurance_fees_generated_amount: u128,
    pub uncollected_solvency_insurance_fees_amount: u128,
    pub liquidation_fees_generated_amount: u128,
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
    pub token_mint_address: Pubkey,
    pub sub_market_index: u16,
    pub fee_collector_address: Pubkey,
    pub fee_on_interest_earned_rate: u16,
    pub supply_interest_change_index: u128, //This index is set to match the token reserve index after previously earned interest is updated. This is only used in the frontend for calculating the 7 day projection rate
    pub borrow_interest_change_index: u128, //This index is set to match the token reserve index after previously accured interest is updated. This is only used in the frontend for calculating the 7 day projection rate
    pub deposited_amount: u128,
    pub interest_earned_amount: u128,
    pub sub_market_fees_generated_amount: u128,
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
pub struct LendingUserAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8, //Giving the lending account an index to allow users to have multiple lending accounts if they so choose, so they don't have to use multiple wallets
    pub account_name: String,
    pub lending_user_account_added: bool,
    pub tab_account_count: u16,
    pub total_deposited_usd_value: u128,
    pub total_borrowed_usd_value: u128,
    pub last_health_update_clock_slot: u64,
    pub temp_deposit_usd_value: u128,
    pub temp_borrow_usd_value: u128,
    pub next_tab_index_to_refresh: u16
}

#[account]
pub struct LendingUserTabAccount
{
    pub bump: u8,
    pub token_mint_address: Pubkey,
    pub token_decimal_amount: u8,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub user_tab_account_index: u16,
    pub user_tab_account_added: bool,
    pub supply_interest_change_index: u128, //This index is set to match the token reserve index after previously earned interest is updated
    pub borrow_interest_change_index: u128, //This index is set to match the token reserve index after previously accured interest is updated
    pub deposited_amount: u64,
    pub interest_earned_amount: u64,
    pub sub_market_fees_generated_amount: u64,
    pub sub_market_fees_collected_amount: u64,
    pub solvency_insurance_fees_generated_amount: u64,
    pub solvency_insurance_fees_collected_amount: u64,
    pub liquidation_fees_generated_amount: u64,
    pub liquidation_fees_collected_amount: u64,
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
    pub token_mint_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub statement_month: u8,
    pub statement_year: u16,
    pub monthly_statement_account_added: bool,
    pub snap_shot_balance_amount: u64,//The snap_shot properties give a snapshot of the value of the Tab Account over its whole life time at the time it is updated
    pub snap_shot_interest_earned_amount: u64,
    pub snap_shot_sub_market_fees_generated_amount: u64,
    pub snap_shot_sub_market_fees_collected_amount: u64,
    pub snap_shot_solvency_insurance_fees_generated_amount: u64,
    pub snap_shot_solvency_insurance_fees_collected_amount: u64,
    pub snap_shot_liquidation_fees_generated_amount: u64,
    pub snap_shot_liquidation_fees_collected_amount: u64,
    pub snap_shot_debt_amount: u64,
    pub snap_shot_interest_accrued_amount: u64,
    pub snap_shot_repaid_debt_amount: u64,
    pub snap_shot_liquidated_amount: u64,
    pub snap_shot_liquidator_amount: u64,
    pub monthly_deposited_amount: u64,//The monthly properties give the specific value changes for that specific month
    pub monthly_interest_earned_amount: u64,
    pub monthly_sub_market_fees_generated_amount: u64,
    pub monthly_sub_market_fees_collected_amount: u64,
    pub monthly_solvency_insurance_fees_generated_amount: u64,
    pub monthly_solvency_insurance_fees_collected_amount: u64,
    pub monthly_liquidation_fees_generated_amount: u64,
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