use anchor_lang::prelude::*;
use anchor_lang::system_program::{self};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenInterface, TokenAccount, TransferChecked, SyncNative, CloseAccount};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;
use ra_solana_math::FixedPoint;
pub mod validation;
pub mod errors;
use crate::validation::*;
use crate::errors::LendingError;

declare_id!("HPSRDRBK5tr9o4gcFzsEZaNLutMqf4SLRdj1BBri3KWa");

#[cfg(not(feature = "no-entrypoint"))] //Ensure it's not included when compiled as a library
security_txt!
{
    name: "M4A Lending Protocol",
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
const INITIAL_LIQUIDATION_TREASURER_ADDRESS: Pubkey = pubkey!("9BRgCdmwyP5wGVTvKAUDjSwucpqGncurVa35DjaWqSsC");//Also the HodlTreasury
#[cfg(feature = "dev")] 
const INITIAL_PRICE_ORACLE_VALIDATOR_ADDRESS: Pubkey = pubkey!("6zpT3Fr3Hw95L23AVgx2D1wFkig8kESXB62dGZHxW2tS");

#[cfg(feature = "local")] 
const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("4FVD4AkuAKCUozYpQFhc1G1ML9dQ5UvyfDhkVbVFvDcn");
#[cfg(feature = "local")] 
const INITIAL_SOLVENCY_TREASURER_ADDRESS: Pubkey = pubkey!("4FVD4AkuAKCUozYpQFhc1G1ML9dQ5UvyfDhkVbVFvDcn");
#[cfg(feature = "local")] 
const INITIAL_LIQUIDATION_TREASURER_ADDRESS: Pubkey = pubkey!("4FVD4AkuAKCUozYpQFhc1G1ML9dQ5UvyfDhkVbVFvDcn");
#[cfg(feature = "local")] 
const INITIAL_PRICE_ORACLE_VALIDATOR_ADDRESS: Pubkey = pubkey!("3jYmEG7Y8fU2696Gqukt95TSNzpkgkYHQsJpypdGW3WE");

const SOL_TOKEN_MINT_ADDRESS: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

//Lending User Account need atleast 4 extra bytes of space to pass with full load(Longest name possible)
const LENDING_USER_ACCOUNT_EXTRA_SIZE: usize = 4;
const INITIAL_MAX_TABS_PER_LENDING_ACCOUNT: u8 = 5;
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
//This way is linear and only compounds the interest when it's called. If a token reserve went months without it being called, that would be a lot of interest lots from lack of compounding.
//The Taylor Series version below is more computationally expensive but compounds the interest more accurately over long periods of time without needing to be called.
//The Taylor Series version is currently being used and the linear version is commented out.
/*fn update_token_reserve_supply_and_borrow_interest_change_index<'info>(token_reserve: &mut TokenReserve, new_time_stamp: u64, new_clock_slot: Option<u64>) -> Result<()>
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
}*/

// Helper function to update Token Reserve Accrued Interest Index using continuous compounding via Taylor Series
fn update_token_reserve_supply_and_borrow_interest_change_index<'info>(
    token_reserve: &mut TokenReserve, 
    new_time_stamp: u64, 
    new_clock_slot: Option<u64>
) -> Result<()> {
    
    //Skip if there is no borrowing in the Token Reserve. There is no interest change if there is no borrowing.
    if token_reserve.borrowed_amount != 0
    {
        // NOTE: Ensure your FixedPoint library has a way to ingest u128 without truncating via `as u64`
        let old_supply_interest_index_fp = FixedPoint::from_scaled_u128(token_reserve.supply_interest_change_index);
        let old_borrow_interest_index_fp = FixedPoint::from_scaled_u128(token_reserve.borrow_interest_change_index);
        
        let number_one_fp = FixedPoint::from_int(1);
        let two_fp = FixedPoint::from_int(2);
        let three_fp = FixedPoint::from_int(3);
        let four_fp = FixedPoint::from_int(4);

        //let supply_apy_fp = FixedPoint::from_bps(token_reserve.supply_apy as u64).map_err(|e| e)?;
        //let borrow_apy_fp = FixedPoint::from_bps(token_reserve.borrow_apy as u64).map_err(|e| e)?;
        let supply_apy_fp = FixedPoint::from_bps(token_reserve.supply_apy as u64)
            .map_err(|_| anchor_lang::prelude::ProgramError::InvalidArgument)?;

        let borrow_apy_fp = FixedPoint::from_bps(token_reserve.borrow_apy as u64)
            .map_err(|_| anchor_lang::prelude::ProgramError::InvalidArgument)?;
        
        let change_in_time = new_time_stamp - token_reserve.last_lending_activity_time_stamp;
        let change_in_time_fp = FixedPoint::from_int(change_in_time);
        let seconds_in_a_year_fp = FixedPoint::from_int(31_556_952); 

        //Taylor Series 4th Order Interest Calculation: e^x = 1 + x + (x^2 / 2!) + (x^3 / 3!) + (x^4 / 4!) 
        //1. Calculate common time factor x: APY * Δt / seconds_in_a_year
        //We multiply by APY first (before dividing) in the steps below to preserve fixed-point precision
        
        //--- SUPPLY INTEREST COMPOUNDING (Taylor Series 4th Order) ---
        let supply_x = supply_apy_fp.mul(&change_in_time_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&seconds_in_a_year_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
        
        let s_term1 = supply_x.clone(); // x
        let s_term2 = s_term1.mul(&supply_x).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&two_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; // x^2 / 2!
        let s_term3 = s_term2.mul(&supply_x).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&three_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; // x^3 / 3!
        let s_term4 = s_term3.mul(&supply_x).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&four_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; // x^4 / 4!
        
        let supply_compounding_factor_fp = number_one_fp
            .add(&s_term1).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .add(&s_term2).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .add(&s_term3).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .add(&s_term4).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

        token_reserve.supply_interest_change_index = old_supply_interest_index_fp.mul(&supply_compounding_factor_fp)
            .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?.value.as_u128();

        //--- BORROW INTEREST COMPOUNDING (Taylor Series 4th Order) ---
        let borrow_x = borrow_apy_fp.mul(&change_in_time_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&seconds_in_a_year_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
        
        let b_term1 = borrow_x.clone(); // x
        let b_term2 = b_term1.mul(&borrow_x).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&two_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; // x^2 / 2!
        let b_term3 = b_term2.mul(&borrow_x).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&three_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; // x^3 / 3!
        let b_term4 = b_term3.mul(&borrow_x).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .div(&four_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; // x^4 / 4!
        
        let borrow_compounding_factor_fp = number_one_fp
            .add(&b_term1).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .add(&b_term2).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .add(&b_term3).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?
            .add(&b_term4).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

        token_reserve.borrow_interest_change_index = old_borrow_interest_index_fp.mul(&borrow_compounding_factor_fp)
            .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?.value.as_u128();

        msg!("Updated Token Reserve Interest Change Indexes");
        msg!("Supply: {}", token_reserve.supply_interest_change_index);
        msg!("Borrow: {}", token_reserve.borrow_interest_change_index);
    }

    token_reserve.last_lending_activity_time_stamp = new_time_stamp;

    //This setting keeps us from running update_token_reserve_supply_and_borrow_interest_change_index more than we need to when calling refresh_user_health_chunk_and_token_reserves
    //It also detects when a user borrows from a token reserve they have never interacted with before
    //Ultimately though, lending_user_account.last_health_update_clock_slot guarantees all relavent token reserves and user tabs have been refreshed for withdraw, borrow, refresh_user_health_chunk_and_token_reserves, and liquidate functions
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
            
            //Borrow APY = Borrow APY Base(Borrow APY Slope1 in this case) + ((Utilization Rate/Optimal Utialization Rate) * Borrow APY Slope1)
            //Setting Borrow APY Base to Borrow APY Slope1 in this case
            if utilization_rate < optimal_utilization_rate
            {
                //Max Borrow Rate = token_reserve.fixed_borrow_apy + token_reserve.fixed_borrow_apy @Less Than 70% Utilization Rate
                let borrow_apy_slope1 = token_reserve.fixed_borrow_apy as u128;
                //Multiply before dividing to help keep precision
                let u_rate_times_borrow_apy_slope1 = utilization_rate * borrow_apy_slope1;
                let u_rate_times_borrow_apy_slope1_divide_optimal_u_rate = u_rate_times_borrow_apy_slope1 / optimal_utilization_rate;

                //Max Borrow Rate = token_reserve.fixed_borrow_apy + token_reserve.fixed_borrow_apy @Less Than 70% Utilization Rate
                token_reserve.borrow_apy = (borrow_apy_slope1 + u_rate_times_borrow_apy_slope1_divide_optimal_u_rate) as u16;
            }
            else
            {
                //Max Borrow Rate = 10% + 34% = 44% @100% Utilization Rate. I think having a rate more than 44% would appear too pay day loany...just seems like a bad look lol.
                let ten_percent = 1_000; //1,000 = 10.00%
                let borrow_apy_slope2 = 3_400; //3,400 = 34.00%

                /*
                * Formula: New High Rate Base = (Current Utilization Rate - Optimal Utilization Rate) / (100% - Optimal Utilization Rate) * Borrow APY Slope 2
                * * This linearly scales the interest rate upward from the 10% base rate at 70% utilization, 
                * reaching the full 34% slope cap (44% total APY) only when utilization hits 100%.
                * * Order of operations: Multiply before dividing to prevent integer truncation / precision loss.
                */
                let u_rate_minus_optimal_u_rate = utilization_rate - optimal_utilization_rate;
                let one_hundred_percent_minus_optimal_u_rate = decimal_scaling - optimal_utilization_rate;
                //Multiply before dividing to help keep precision
                let u_rate_minus_optimal_u_rate_times_borrow_apy_slope2 = u_rate_minus_optimal_u_rate * borrow_apy_slope2;
                let new_high_rate_base = u_rate_minus_optimal_u_rate_times_borrow_apy_slope2 / one_hundred_percent_minus_optimal_u_rate;

                //Max Borrow Rate = 10% + 34% = 44% @100% Utilization Rate.
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
    let old_user_balance_mul_token_reserve_index_fp = old_user_deposited_amount_fp.mul(&token_reserve_supply_index_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_user_deposited_amount_before_fees_fp = old_user_balance_mul_token_reserve_index_fp.div(&user_supply_index_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_user_interest_earned_amount_before_fees_fp = new_user_deposited_amount_before_fees_fp.sub(&old_user_deposited_amount_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

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
    let sub_market_fee_rate_fp = FixedPoint::from_bps(sub_market_fee as u64).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_sub_market_fees_generated_amount_before_round = new_user_interest_earned_amount_before_fees_fp.mul(&sub_market_fee_rate_fp)
        .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; //Taking the floor before subtraction prevents the token reserve from having extra deposit amounts. Although having an extra deposit amount can act as a safety buffer for liquidity when there is bad debt, that's what the solvency insurance fee is for.
    let new_sub_market_fees_generated_amount_fp_floor = (new_sub_market_fees_generated_amount_before_round/*.add(&round_up_at_point_5)?*/).floor();
    let new_sub_market_fees_generated_amount = new_sub_market_fees_generated_amount_fp_floor.to_u128().map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

    //Calculate Solvency Insurance Fee
    let solvency_insurance_fee_rate_fp = FixedPoint::from_bps(solvency_insurance_fee as u64).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_solvency_insurance_fees_generated_amount_before_round = new_user_interest_earned_amount_before_fees_fp.mul(&solvency_insurance_fee_rate_fp)
        .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?; //Taking the floor before subtraction prevents the token reserve from having extra deposit amounts. Although having an extra deposit amount can act as a safety buffer for liquidity when there is bad debt, that's what the solvency insurance fee is for.
    let new_solvency_insurance_fees_generated_amount_fp_floor = (new_solvency_insurance_fees_generated_amount_before_round/*.add(&round_up_at_point_5)?*/).floor();
    let mut new_solvency_insurance_fees_generated_amount = new_solvency_insurance_fees_generated_amount_fp_floor.to_u128().map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

    //Apply Fees to Interest Earned
    let new_user_interest_earned_amount_after_sb_fee_fp = new_user_interest_earned_amount_before_fees_fp.sub(&new_sub_market_fees_generated_amount_fp_floor)
        .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_user_interest_earned_amount_after_fees_fp = new_user_interest_earned_amount_after_sb_fee_fp.sub(&new_solvency_insurance_fees_generated_amount_fp_floor)
        .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let mut new_user_interest_earned_amount_after_fees = new_user_interest_earned_amount_after_fees_fp.to_u128()
        .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

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
    token_reserve.uncollected_solvency_insurance_fees_amount += new_solvency_insurance_fees_generated_amount;
    sub_market.deposited_amount += new_user_interest_earned_amount_after_fees;
    sub_market.interest_earned_amount += new_user_interest_earned_amount_after_fees;
    sub_market.sub_market_fees_generated_amount += new_sub_market_fees_generated_amount;
    sub_market.uncollected_sub_market_fees_amount += new_sub_market_fees_generated_amount;
    sub_market.solvency_insurance_fees_generated_amount += new_solvency_insurance_fees_generated_amount;
    lending_user_tab_account.deposited_amount += new_user_interest_earned_amount_after_fees as u64;
    lending_user_tab_account.interest_earned_amount += new_user_interest_earned_amount_after_fees as u64;
    lending_user_tab_account.fees_generated_amount += new_sub_market_fees_generated_amount as u64;
    lending_user_tab_account.fees_generated_amount += new_solvency_insurance_fees_generated_amount as u64;
    lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
    lending_user_monthly_statement_account.monthly_interest_earned_amount += new_user_interest_earned_amount_after_fees as u64;
    lending_user_monthly_statement_account.fees_generated_amount += new_sub_market_fees_generated_amount as u64;
    lending_user_monthly_statement_account.fees_generated_amount += new_solvency_insurance_fees_generated_amount as u64;

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
    let old_user_debt_mul_token_reserve_index_fp = old_user_borrowed_amount_fp.mul(&token_reserve_borrow_index_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_user_borrowed_amount_fp = old_user_debt_mul_token_reserve_index_fp.div(&user_borrow_index_fp).map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_user_interest_accrued_amount_fp = (new_user_borrowed_amount_fp.sub(&old_user_borrowed_amount_fp)
    .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?).ceil()
    .map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;
    let new_user_interest_accrued_amount = new_user_interest_accrued_amount_fp.to_u128().map_err(|_| anchor_lang::prelude::ProgramError::ArithmeticOverflow)?;

    token_reserve.borrowed_amount += new_user_interest_accrued_amount;
    token_reserve.interest_accrued_amount += new_user_interest_accrued_amount;
    sub_market.borrowed_amount += new_user_interest_accrued_amount;
    sub_market.interest_accrued_amount += new_user_interest_accrued_amount;
    lending_user_tab_account.borrowed_amount += new_user_interest_accrued_amount as u64;
    lending_user_tab_account.interest_accrued_amount += new_user_interest_accrued_amount as u64;
    lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
    lending_user_monthly_statement_account.monthly_interest_accrued_amount += new_user_interest_accrued_amount as u64;

    Ok(())
}

fn check_token_price_staleness(price_data_clock_slot: u64, current_clock_slot: u64) -> Result<()>
{
    #[cfg(feature = "local")] 
    //Allow a max age of 40 slots (approx 16 seconds) 1 slot is about 400ms
    if current_clock_slot.saturating_sub(price_data_clock_slot) > 7//40
    {
        msg!("Current Slot: {}", current_clock_slot);
        msg!("Data Slot: {}", price_data_clock_slot);
        return Err(error!(LendingError::OracleDataStale));
    }

    #[cfg(feature = "dev")]
    //Allow a max age of 0 slots (approx 0 seconds)
    if current_clock_slot.saturating_sub(price_data_clock_slot) > 3
    {
        msg!("Current Slot: {}", current_clock_slot);
        msg!("Data Slot: {}", price_data_clock_slot);
        return Err(error!(LendingError::OracleDataStale));
    }

    Ok(())
}

fn refund_oracle_temp_account_fees(temp_price_account_info: &AccountInfo, oracle_account_info: &AccountInfo)
{
    //Refund price fee Lamports (Rent) back to the oracle
    let dest_starting_lamports = oracle_account_info.lamports();
    **oracle_account_info.lamports.borrow_mut() = dest_starting_lamports
        .checked_add(temp_price_account_info.lamports())
        .unwrap();
    **temp_price_account_info.lamports.borrow_mut() = 0;

    let mut temp_data = temp_price_account_info.data.borrow_mut();
    temp_data.fill(0);

    //Zero out the data and reassign to System Program (Standard Solana cleanup)
    temp_price_account_info.assign(&system_program::ID);
}

fn get_verified_token_price(verified_token_prices: &[VerifiedPriceData], token_id: u8) -> Result<u128>
{
    //Search the slice for the first item matching the target token_id
    let found_data = verified_token_prices
        .iter()
        .find(|data| data.token_id == token_id);

    match found_data
    {
        Some(data) => Ok(data.normalized_price_18_decimals),
        None =>
        {
            msg!("🚨 Requested Token ID not found in verified prices: {}", token_id);
            Err(error!(LendingError::OraclePriceNotFound))
        }
    }
}

//Helper function to initialize Lending User Account
fn initialize_lending_user_account<'info>(lending_user_account: &mut LendingUserAccount,
    bump: u8,
    user_account_owner: Pubkey,
    user_account_index: u8,
    account_name: String,
    look_up_table_address: Pubkey
) -> Result<()>
{
    //Account Name string must not be longer than 25 characters
    require!(account_name.len() <= MAX_ACCOUNT_NAME_LENGTH, LendingError::LendingUserAccountNameTooLong);

    lending_user_account.bump = bump;
    lending_user_account.owner = user_account_owner;
    lending_user_account.user_account_index = user_account_index;
    lending_user_account.account_name = account_name.clone();
    lending_user_account.look_up_table_address = look_up_table_address;
    lending_user_account.lending_user_account_added = true;

    msg!("Created Lending User Account Named: {}", account_name);
    msg!("Updated Lending User Look Up Table Address: {}", lending_user_account.look_up_table_address);

    Ok(())
}

//Helper function to initialize Lending User Tab Account
fn initialize_lending_user_tab_account<'info>(lending_user_account: &mut LendingUserAccount,
    lending_user_tab_account: &mut LendingUserTabAccount,
    lending_protocol: &LendingProtocol,
    bump: u8,
    token_id: u8,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner: Pubkey,
    user_account_index: u8
) -> Result<()>
{
    lending_user_tab_account.bump = bump;
    lending_user_tab_account.token_id = token_id;
    lending_user_tab_account.sub_market_owner_address = sub_market_owner_address;
    lending_user_tab_account.sub_market_index = sub_market_index;
    lending_user_tab_account.user_tab_account_index = lending_user_account.tab_account_count;
    lending_user_tab_account.owner = user_account_owner;
    lending_user_tab_account.user_account_index = user_account_index;
    lending_user_tab_account.user_tab_account_added = true;

    lending_user_account.tab_account_count += 1;

    //Limit the number of tab accounts to prevent accounts from becoming broken with too many tab accounts.
    //Unable to withdraw, borrow, repay, or be liquidated because too many transactions would be required to land in the same slot.
    //Jito bundles only allow for up to 5 transacations.
    //A user can create a new account if there are other submarkets they want to interact with
    //I've tested things out with 5 tokens and will increase as I eventually try to add more tokens to try and let the user keep all the tokens in the same account. (assuming they aren't joining a bunch of different submarkets and trying to break things, lol.
    //That's why ideally the max amount is equal the number of different tokens, but doesn't have to be)
    require!(lending_user_account.tab_account_count <= lending_protocol.max_tabs_per_lending_account, LendingError::TooManyTabAccounts);

    msg!("Created Lending User Tab Account Indexed At: {}", lending_user_tab_account.user_tab_account_index);

    Ok(())
}

//Helper function to initialize Monthly Statement Account
fn initialize_lending_user_monthly_statement_account<'info>(lending_user_monthly_statement_account: &mut LendingUserMonthlyStatementAccount,
    lending_user_tab_account: &LendingUserTabAccount,
    lending_protocol: &LendingProtocol,
    bump: u8,
    token_id: u8,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner: Pubkey,
    user_account_index: u8
) -> Result<()>
{
    lending_user_monthly_statement_account.bump = bump;
    lending_user_monthly_statement_account.token_id = token_id;
    lending_user_monthly_statement_account.sub_market_owner_address = sub_market_owner_address;
    lending_user_monthly_statement_account.sub_market_index = sub_market_index;
    lending_user_monthly_statement_account.owner = user_account_owner;
    lending_user_monthly_statement_account.user_account_index = user_account_index;
    lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
    lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
    lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;
    lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;
    lending_user_monthly_statement_account.monthly_statement_account_added = true;

    msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);

    Ok(())
}

fn deposit_tokens_into_token_reserve_from_user<'info>(token_mint_address: Pubkey,
    token_reserve_ata_info: &AccountInfo<'info>,
    user_ata_info: &AccountInfo<'info>,
    token_mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    signer: &Signer<'info>,
    system_program_account: &Program<'info, System>,
    transfer_amount: u64,
    should_close_ata: bool
) -> Result<()>
{
    //Handle native SOL transactions
    if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
    {
        //CPI to the System Program to transfer SOL from the user to the program's wSOL ATA.
        let cpi_accounts = system_program::Transfer
        {
            from: signer.to_account_info(),
            to: token_reserve_ata_info.clone()
        };
        let cpi_program = system_program_account.key();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        system_program::transfer(cpi_ctx, transfer_amount)?;

        //CPI to the SPL Token Program to "sync" the wSOL ATA's balance.
        let cpi_accounts = SyncNative
        {
            account: token_reserve_ata_info.clone(),
        };
        let cpi_program = token_program.key();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token_interface::sync_native(cpi_ctx)?;

        //Close temporary wSOL ATA if its balance is zero
        if should_close_ata
        {
            //Since the User has no other wrapped SOL, close the temporary wrapped SOL account
            let cpi_accounts = CloseAccount
            {
                account: user_ata_info.clone(),
                destination: signer.to_account_info(),
                authority: signer.to_account_info(),
            };
            let cpi_program = token_program.key();
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
            from: user_ata_info.clone(),
            to: token_reserve_ata_info.clone(),
            mint: token_mint.to_account_info(),
            authority: signer.to_account_info()
        };
        let cpi_program = token_program.key();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        //Transfer Tokens Into The Reserve
        token_interface::transfer_checked(cpi_ctx, transfer_amount, token_mint.decimals)?;  
    }

    Ok(())
}

fn withdraw_tokens_from_token_reserve_to_user<'info>(token_mint_address: Pubkey,
    token_reserve: &Account<'info, TokenReserve>,
    token_reserve_ata_info: &AccountInfo<'info>,
    user_ata_info: &AccountInfo<'info>,
    token_mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    signer: &Signer<'info>,
    system_program_account: &Program<'info, System>,
    transfer_amount: u64,
    should_close: bool
) -> Result<()>
{
    let seeds = &[b"tokenReserve", token_mint_address.as_ref(), &[token_reserve.bump]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = TransferChecked
    {
        from: token_reserve_ata_info.clone(),
        to: user_ata_info.clone(),
        mint: token_mint.to_account_info(),
        authority: token_reserve.to_account_info()
    };
    let cpi_program = token_program.key();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

    //Transfer Tokens Back to the User
    token_interface::transfer_checked(cpi_ctx, transfer_amount, token_mint.decimals)?;

    //Handle wSOL Token unwrap
    if token_mint_address.key() == SOL_TOKEN_MINT_ADDRESS.key()
    {
        if !should_close
        {
            //Since User already had wrapped SOL, only unwrapped the amount withdrawn
            let cpi_accounts = system_program::Transfer
            {
                from: user_ata_info.clone(),
                to: signer.to_account_info()
            };
            let cpi_program = system_program_account.key();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            system_program::transfer(cpi_ctx, transfer_amount)?;
        }
        else
        {
            //Since the User has no other wrapped SOL, unwrap it all, send it to the User, and close the temporary wrapped SOL account
            let cpi_accounts = CloseAccount
            {
                account: user_ata_info.clone(),
                destination: signer.to_account_info(),
                authority: signer.to_account_info(),
            };
            let cpi_program = token_program.key();
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
        require_keys_eq!(ctx.accounts.signer.key(), INITIAL_CEO_ADDRESS, LendingError::NotCEO);

        let ceo = &mut ctx.accounts.ceo;
        ceo.address = INITIAL_CEO_ADDRESS;

        let solvency_treasurer = &mut ctx.accounts.solvency_treasurer;
        solvency_treasurer.address = INITIAL_SOLVENCY_TREASURER_ADDRESS;

        let liquidation_treasurer = &mut ctx.accounts.liquidation_treasurer;
        liquidation_treasurer.address = INITIAL_LIQUIDATION_TREASURER_ADDRESS;

        let price_validator = &mut ctx.accounts.price_validator;
        price_validator.bump = ctx.bumps.price_validator;
        price_validator.address = INITIAL_PRICE_ORACLE_VALIDATOR_ADDRESS;

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.current_statement_month = statement_month;
        lending_protocol.current_statement_year = statement_year;
        lending_protocol.max_tabs_per_lending_account = INITIAL_MAX_TABS_PER_LENDING_ACCOUNT;
        lending_protocol.look_up_table_address = ctx.accounts.look_up_table_address.key();

        let lending_stats = &mut ctx.accounts.lending_stats;
        lending_stats.bump = ctx.bumps.lending_stats;

        msg!("Lending Protocol Initialized");
        msg!("New CEO Address: {}", ceo.address.key());
        msg!("Current Statement Month: {}, Year: {}", lending_protocol.current_statement_month, lending_protocol.current_statement_year);
        msg!("Lending Protocol Look Up Table: {}", lending_protocol.look_up_table_address);

        Ok(())
    }

    pub fn pass_on_lending_protocol_ceo(ctx: Context<PassOnLendingProtocolCEO>) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        msg!("The Lending Protocol CEO has passed on the title to a new CEO");
        msg!("New CEO: {}", ctx.accounts.new_ceo_address.key());

        ceo.address = ctx.accounts.new_ceo_address.key();

        Ok(())
    }

    pub fn pass_on_solvency_treasurer(ctx: Context<PassOnSolvencyTreasurer>) -> Result<()> 
    {
        let solvency_treasurer = &mut ctx.accounts.solvency_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), solvency_treasurer.address.key(), LendingError::NotSolvencyTreasurer);

        msg!("The Solvency Treasurer has passed on the title to a new Treasurer");
        msg!("New Treasurer: {}", ctx.accounts.new_treasurer_address.key());

        solvency_treasurer.address = ctx.accounts.new_treasurer_address.key();

        Ok(())
    }

    pub fn pass_on_liquidation_treasurer(ctx: Context<PassOnLiquidationTreasurer>) -> Result<()> 
    {
        let liquidation_treasurer = &mut ctx.accounts.liquidation_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), liquidation_treasurer.address.key(), LendingError::NotLiquidationTreasurer);

        msg!("The Liquidation Treasurer has passed on the title to a new Treasurer");
        msg!("New Treasurer: {}", ctx.accounts.new_treasurer_address.key());

        liquidation_treasurer.address = ctx.accounts.new_treasurer_address.key();

        Ok(())
    }

    pub fn set_oracle_price_validator(ctx: Context<SetOraclePriceValidator>) -> Result<()> 
    {
        let ceo = &ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        let price_validator = &mut ctx.accounts.price_validator;

        msg!("A new Oracle Price Validator has been set");
        msg!("New Price Validator: {}", ctx.accounts.new_price_validator_address.key());

        price_validator.address = ctx.accounts.new_price_validator_address.key();

        Ok(())
    }

    pub fn create_temp_oracle_price_data(ctx: Context<CreateTempOraclePriceData>, payload: PriceDataPayload) -> Result<()> 
    {
        let price_validator = &ctx.accounts.price_validator;
        //Only the Price Oracle can call this function
        require_keys_eq!(ctx.accounts.signer.key(), price_validator.address.key(), LendingError::NotPriceOracle);

        let temp_price_account = &mut ctx.accounts.temp_price_account;

        temp_price_account.bump = ctx.bumps.temp_price_account;
        temp_price_account.data = payload.data;
        temp_price_account.slot = payload.slot;

        Ok(())
    }

    pub fn close_temp_oracle_price_data(ctx: Context<CloseTempOraclePriceData>) -> Result<()> 
    {
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        let price_validator = &ctx.accounts.price_validator;
        let temp_price_account_info = ctx.accounts.temp_price_account.to_account_info();

        //Refund Oracle price account fees back to Oracle
        let oracle_account_info = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        require_keys_eq!(oracle_account_info.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
        
        //1. Snapshot the balance
        let rent_lamports = temp_price_account_info.lamports();

        //2. Perform the safe math balance transfer
        **oracle_account_info.lamports.borrow_mut() = oracle_account_info.lamports()
            .checked_add(rent_lamports)
            .unwrap();
            
        **temp_price_account_info.lamports.borrow_mut() = 0;

        //3. Clear out the anchor discriminator layout so the account can't be reused within the same slot block
        let mut data = temp_price_account_info.try_borrow_mut_data()?;
        data.fill(0);

        Ok(())
    }

    pub fn update_current_statement_month_and_year(ctx: Context<UpdateLendingProtocol>, statement_month: u8, statement_year: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.current_statement_month = statement_month;
        lending_protocol.current_statement_year = statement_year;

        msg!("Updated Lending Protocol To Statement Month: {}, Year: {}", lending_protocol.current_statement_month, lending_protocol.current_statement_year);

        Ok(())
    }

    pub fn update_max_tab_amount(ctx: Context<UpdateLendingProtocol>, new_max_tab_amount: u8) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.max_tabs_per_lending_account = new_max_tab_amount;

        msg!("Updated Lending Protocol Max Tabs To: {}", new_max_tab_amount);

        Ok(())
    }

    pub fn add_token_reserve(ctx: Context<AddTokenReserve>,
        token_decimal_amount: u8,
        fixed_borrow_apy: u16,
        use_fixed_borrow_apy: bool,
        global_limit: u128,
        solvency_insurance_fee_rate: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        //Solvency Insurance Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(solvency_insurance_fee_rate <= 10_000, LendingError::InvalidSolvencyInsuranceFeeRate);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.bump = ctx.bumps.token_reserve;
        token_reserve.token_mint_address = ctx.accounts.token_mint.key();
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.borrow_apy = fixed_borrow_apy;
        token_reserve.fixed_borrow_apy = fixed_borrow_apy;
        token_reserve.use_fixed_borrow_apy = use_fixed_borrow_apy;
        token_reserve.global_limit = global_limit;
        token_reserve.solvency_insurance_fee_rate = solvency_insurance_fee_rate;
        token_reserve.supply_interest_change_index = 1_000_000_000_000_000_000;
        token_reserve.borrow_interest_change_index = 1_000_000_000_000_000_000;

        token_reserve_stats.token_reserve_count += 1;
        token_reserve.token_id = token_reserve_stats.token_reserve_count;
        
        msg!("Added Token Reserve #{}", token_reserve_stats.token_reserve_count);
        msg!("Token Mint Address: {}", ctx.accounts.token_mint.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
        msg!("Fixed Borrow APY: {}", fixed_borrow_apy);
        msg!("Use fixed Borrow APY: {}", use_fixed_borrow_apy);
        msg!("Global Limit: {}", global_limit);
            
        Ok(())
    }

    pub fn update_token_reserve(ctx: Context<UpdateTokenReserve>,
        fixed_borrow_apy: u16,
        use_fixed_borrow_apy: bool,
        global_limit: u128,
        solvency_insurance_fee_rate: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        //Solvency Insurance Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(solvency_insurance_fee_rate <= 10_000, LendingError::InvalidSolvencyInsuranceFeeRate);

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
        msg!("Token ID: {}", token_reserve.token_id);
        msg!("New Fixed Borrow APY: {}", fixed_borrow_apy);
        msg!("New Global Limit: {}",  global_limit);
            
        Ok(())
    }

    pub fn create_sub_market(ctx: Context<CreateSubMarket>,
        sub_market_index: u16,
        fee_on_interest_earned_rate: u16,
        look_up_table_address: Option<Pubkey> //Needed when a user creates their first Sub Market
    ) -> Result<()> 
    {
        //SubMarket Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(fee_on_interest_earned_rate <= 10_000, LendingError::InvalidSubMarketFeeRate);

        let token_reserve = &ctx.accounts.token_reserve;

        let sub_market = &mut ctx.accounts.sub_market;
        sub_market.bump = ctx.bumps.sub_market;
        sub_market.owner = ctx.accounts.signer.key();
        sub_market.fee_collector_address = ctx.accounts.fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate; //This should fed in fixed point notation from 0 to 10,000 (0 to 100%)
        sub_market.token_id = token_reserve.token_id; //This can't be edited after. Allowing this to be edited would be like allowing some one to say this currency is a different kind of currency later when ever they wanted
        sub_market.sub_market_index = sub_market_index;
        
        let sub_market_stats = &mut ctx.accounts.sub_market_stats;
        sub_market_stats.sub_market_creation_count += 1;
        sub_market.id = sub_market_stats.sub_market_creation_count;

        msg!("Created SubMarket #{}", sub_market.id);
        msg!("Token ID: {}", sub_market.token_id);
        msg!("SubMarket Index: {}", sub_market.sub_market_index);
        msg!("Owner: {}", ctx.accounts.signer.key());
        msg!("Fee Collector Address: {}", ctx.accounts.fee_collector_address.key());
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate as f64 / 100.0); //convert from fixed point notation with 4 decimal places back to decimal for logging
        
        //Add Look Up Table Address to account if being newly initialized.
        let sub_market_owner_look_up_table = &mut ctx.accounts.sub_market_owner_look_up_table;
        if sub_market_owner_look_up_table.look_up_table_added == false
        {
            let lut_address = look_up_table_address.ok_or(LendingError::MissingSubMarketLookUpTable)?;

            sub_market_owner_look_up_table.owner = ctx.accounts.signer.key();
            sub_market_owner_look_up_table.look_up_table_address = lut_address;
            sub_market_owner_look_up_table.look_up_table_added = true;
            msg!("Created Sub Market Owner Look Up Table: {}", sub_market_owner_look_up_table.look_up_table_address.key());
        }

        Ok(())
    }

    pub fn edit_sub_market(ctx: Context<EditSubMarket>,
        token_id: u8,
        sub_market_index: u16,
        fee_on_interest_earned_rate: u16
    ) -> Result<()> 
    {
        //SubMarket Fee on interest earned rate can't be greater than 100%, 1 in decimal form, 10,000 in fixed point notation
        require!(fee_on_interest_earned_rate <= 10_000, LendingError::InvalidSubMarketFeeRate);

        let sub_market = &mut ctx.accounts.sub_market;
        sub_market.fee_collector_address = ctx.accounts.fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate;

        let sub_market_stats = &mut ctx.accounts.sub_market_stats;
        sub_market_stats.sub_market_edit_count += 1;
        
        msg!("Edited Submarket");
        msg!("Token ID: {}", token_id);
        msg!("SubMarket Index: {}", sub_market_index);
        msg!("Owner: {}", ctx.accounts.signer.key());
        msg!("Fee Collector Address: {}", ctx.accounts.fee_collector_address.key());
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate as f64 / 100.0); //convert from fixed point notation with 4 decimal places back to decimal for logging
            
        Ok(())
    }

    //Looking to see if this isn't necessary
    /*pub fn update_lending_user_look_up_table_address(ctx: Context<UpdateLendingUserLookUpTableAddress>, look_up_table_address: Pubkey) -> Result<()> 
    {
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        lending_user_account.look_up_table_address = look_up_table_address;

        msg!("Updated Lending User Look Up Table Address: {}", lending_user_account.look_up_table_address);

        Ok(())
    }*/

    pub fn deposit_tokens(ctx: Context<DepositTokens>,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
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
        require!(new_token_reserve_deposited_amount <= token_reserve.global_limit, LendingError::GlobalLimitExceeded);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();

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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                lending_user_account,
                ctx.bumps.lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }
        
        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_tab_account,
                token_reserve.token_id,
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
                token_reserve.token_id,
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

        let user_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.user_ata.to_account_info().data.borrow()[..])?;
        let should_close = user_ata_data.amount == 0;
        deposit_tokens_into_token_reserve_from_user(
            ctx.accounts.token_mint.key(),
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.user_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            amount,
            should_close
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

        msg!("{} deposited at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
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
        require!(account_name.len() <= MAX_ACCOUNT_NAME_LENGTH, LendingError::LendingUserAccountNameTooLong);

        let lending_user_account = &mut ctx.accounts.lending_user_account;
        lending_user_account.account_name = account_name.clone();

        let lending_user_stats = &mut ctx.accounts.lending_user_stats;
        lending_user_stats.name_change_count += 1;

        msg!("Lending User Account name updated to: {}", account_name);

        Ok(()) 
    }

    //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
    pub fn withdraw_tokens(ctx: Context<WithdrawTokens>,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        withdraw_max: bool
    ) -> Result<()> 
    {
        let lending_stats = &mut ctx.accounts.lending_stats;
        let price_validator = &ctx.accounts.price_validator;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();

        //This keeps users who have no debt at all from needing to check prices on withdrawals
        if lending_user_account.total_borrowed_usd_value > 0
        {
            //This withdraw_tokens function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s) if the user has debt
            require!(lending_user_account.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserveOrLendingUser);
        }
        else
        {
            //Initialize monthly statement account if the statement month/year has changed.
            if lending_user_monthly_statement_account.monthly_statement_account_added == false
            {
                let lending_protocol = &ctx.accounts.lending_protocol;
                initialize_lending_user_monthly_statement_account(
                    lending_user_monthly_statement_account,
                    lending_user_tab_account,
                    lending_protocol,
                    ctx.bumps.lending_user_monthly_statement_account,
                    token_reserve.token_id,
                    sub_market_owner_address.key(),
                    sub_market_index,
                    ctx.accounts.signer.key(),
                    user_account_index,
                )?;
            }

            let time_stamp = Clock::get()?.unix_timestamp as u64;

            //Calculate Token Reserve Previously Earned And Accrued Interest
            update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;

            update_user_previous_interest_earned(
                token_reserve,
                sub_market,
                lending_user_tab_account,
                lending_user_monthly_statement_account
            )?;
        }

        //After updating interest earned and accrued, set withdraw amount
        let token_reserve_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.token_reserve_ata.to_account_info().data.borrow()[..])?;
        let token_reserve_available_amount = token_reserve_ata_data.amount;
        let mut withdraw_amount;

        if withdraw_max
        {
            withdraw_amount = std::cmp::min(token_reserve_available_amount, lending_user_tab_account.deposited_amount);
        }
        else
        {
            withdraw_amount = amount
        }

        //Skip if user has no debt
        if lending_user_account.total_borrowed_usd_value > 0
        {
            ////////////////////////////
            //Validate Oracle Price Data
            let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
            let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
                temp_price_account_serialized,
                ctx.accounts.signer.key())?;

            check_token_price_staleness(temp_price_account.slot, clock_slot)?;
            
            let normalized_price_18_decimals = get_verified_token_price(&temp_price_account.data, token_reserve.token_id)?;
            let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 

            if !withdraw_max
            {
                let new_user_deposited_usd_value = lending_user_account.total_deposited_usd_value - ((withdraw_amount as u128 * normalized_price_18_decimals) / token_conversion_number);
                
                //Multiply before dividing to help keep precision
                let seventy_percent_of_new_deposited_usd_value = (new_user_deposited_usd_value * 70) / 100;

                //You can't withdraw an amount that would cause your borrow liabilities to exceed 70% of deposited collateral.
                require!(seventy_percent_of_new_deposited_usd_value >= lending_user_account.total_borrowed_usd_value, LendingError::LiquidationExposure);
            }
            else
            {
                //1. Calculate the exact floor amount of USD collateral that MUST remain behind to maintain a 70% LTV
                let min_required_deposited_usd_value = (lending_user_account.total_borrowed_usd_value * 100) / 70;

                if lending_user_account.total_deposited_usd_value > min_required_deposited_usd_value 
                {
                    //2. Find out how much total USD value the user can safely strip out
                    let max_withdraw_usd_value = lending_user_account.total_deposited_usd_value - min_required_deposited_usd_value;

                    //3. Convert that safe USD allowance back into native token units using the oracle price
                    let max_allowed_token_withdraw = (max_withdraw_usd_value * token_conversion_number) / normalized_price_18_decimals;

                    //4. Cap it by the user's absolute token balance in this tab and token reserve liquidity amount
                    let safe_max_tokens = std::cmp::min(max_allowed_token_withdraw, lending_user_tab_account.deposited_amount as u128) as u64;
                    withdraw_amount = std::cmp::min(safe_max_tokens, token_reserve_available_amount);
                } 
                else 
                {
                    //User is already at or exceeding 70% LTV, they cannot withdraw anything safely.
                    return Err(LendingError::LiquidationExposure.into());
                }
            }
            
            //Refund Oracle price account fees back to Oracle
            let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
            refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);
        }

        //You can't withdraw more funds than you've deposited
        require!(lending_user_tab_account.deposited_amount >= withdraw_amount, LendingError::InsufficientFunds);

        //You can't withdraw or borrow more funds than are currently available in the Token Reserve. This can happen if there is too much borrowing going on.
        require!(token_reserve_available_amount >= withdraw_amount, LendingError::InsufficientLiquidity);

        let user_token_data = TokenAccount::try_deserialize(&mut &ctx.accounts.user_ata.to_account_info().data.borrow()[..])?;
        let balance_after_withdrawal = user_token_data.amount.saturating_sub(withdraw_amount);
        let should_close = balance_after_withdrawal == 0;
        withdraw_tokens_from_token_reserve_to_user(
            ctx.accounts.token_mint.key(),
            token_reserve,
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.user_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            withdraw_amount,
            should_close
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
        
        msg!("{} withdrew at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
        sub_market_owner_address.key(),
        sub_market_index);

        Ok(())
    }

    //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
    pub fn borrow_tokens(ctx: Context<BorrowTokens>,
        sub_market_index: u16,
        user_account_index: u8,
        amount: u64,
        borrow_max: bool
    ) -> Result<()> 
    {
        let lending_stats = &mut ctx.accounts.lending_stats;
        let price_validator = &ctx.accounts.price_validator;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;

        //The borrow_tokens function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        if token_reserve.last_health_update_clock_slot != clock_slot
        {
            let time_stamp = Clock::get()?.unix_timestamp as u64;
            
            //When a user is borrowing from a token reserve they have never interacted with before, it won't get refreshed by refresh_user_health_chunk, so doing it here
            update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;
        }
        
        require!(lending_user_account.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserveOrLendingUser);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        //This is for when a user is borrowing a token they have never interacted with before
        if lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_tab_account,
                token_reserve.token_id,
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }
        //This is for when a user is borrowing a token they have never interacted with before
        //You won't be able to use the create_new_monthly_statement until after the lending_user_tab_account exists
        //Normally create_new_monthly_statement and refresh_user_health_chunk would have this covered
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_monthly_statement_account(
                lending_user_monthly_statement_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_monthly_statement_account,
                token_reserve.token_id,
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        ////////////////////////////
        //Validate Oracle Price Data
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
            temp_price_account_serialized,
            ctx.accounts.signer.key())?;

        check_token_price_staleness(temp_price_account.slot, clock_slot)?;

        let normalized_price_18_decimals = get_verified_token_price(&temp_price_account.data, token_reserve.token_id)?;
        let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 

        //Determine Borrow Amount
        let token_reserve_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.token_reserve_ata.to_account_info().data.borrow()[..])?;
        let token_reserve_available_amount = token_reserve_ata_data.amount;
        let max_total_allowed_debt_usd_value = (lending_user_account.total_deposited_usd_value * 70) / 100;
        let mut borrow_amount = amount;

        if !borrow_max 
        {
            //You can't borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral.
            lending_user_account.total_borrowed_usd_value += (borrow_amount as u128 * normalized_price_18_decimals) / token_conversion_number;
            require!(max_total_allowed_debt_usd_value >= lending_user_account.total_borrowed_usd_value, LendingError::LiquidationExposure);
        }
        else
        {
            if max_total_allowed_debt_usd_value > lending_user_account.total_borrowed_usd_value 
            {
                //1. Determine available headroom remaining in USD
                let remaining_usd_borrow_headroom = max_total_allowed_debt_usd_value - lending_user_account.total_borrowed_usd_value;

                //2. Convert USD target capacity into native token fractions using the oracle price
                let max_tokens_allowed = (remaining_usd_borrow_headroom * token_conversion_number) / normalized_price_18_decimals;

                //3. Cap it by the user's max allowed amount and token reserve liquidity amount
                borrow_amount = std::cmp::min(max_tokens_allowed, token_reserve_available_amount as u128) as u64;
                
                //4. Update global account trackers with finalized calculations
                lending_user_account.total_borrowed_usd_value += (borrow_amount as u128 * normalized_price_18_decimals) / token_conversion_number;
                require!(max_total_allowed_debt_usd_value >= lending_user_account.total_borrowed_usd_value, LendingError::LiquidationExposure);
            }
            else
            {
                //User is already at or exceeding 70% LTV, they cannot borrow anything safely.
                return Err(LendingError::LiquidationExposure.into());
            }
        }

        //You can't withdraw or borrow more funds than are currently available in the Token Reserve. This can happen if there is too much borrowing going on.
        require!(token_reserve_available_amount >= borrow_amount, LendingError::InsufficientLiquidity);

        //Refund Oracle price account fees back to Oracle
        let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
        refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);

        let user_token_data = TokenAccount::try_deserialize(&mut &ctx.accounts.user_ata.to_account_info().data.borrow()[..])?;
        let balance_after_withdrawal = user_token_data.amount.saturating_sub(borrow_amount);
        let should_close = balance_after_withdrawal == 0;
        withdraw_tokens_from_token_reserve_to_user(
            ctx.accounts.token_mint.key(),
            token_reserve,
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.user_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            borrow_amount,
            should_close
        )?;

        //Update Values and Stat Listener
        lending_stats.borrows += 1;
        sub_market.borrowed_amount += borrow_amount as u128;
        token_reserve.borrowed_amount += borrow_amount as u128;
        lending_user_tab_account.borrowed_amount += borrow_amount;
        lending_user_monthly_statement_account.monthly_borrowed_amount += borrow_amount;
        lending_user_monthly_statement_account.snap_shot_debt_amount = lending_user_tab_account.borrowed_amount;

        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(token_reserve)?;
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = borrow_amount;
        token_reserve.last_lending_activity_type = Activity::Borrow as u8;
        sub_market.last_lending_activity_amount = borrow_amount;
        sub_market.last_lending_activity_type = Activity::Borrow as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp; 
        lending_user_monthly_statement_account.last_lending_activity_amount = borrow_amount;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Borrow as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        
        msg!("{} borrowed at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
        sub_market_owner_address.key(),
        sub_market_index);

        Ok(())
    }

    pub fn repay_tokens(ctx: Context<RepayTokens>,
        sub_market_index: u16,
        _user_account_index: u8,
        amount: u64,
        pay_off_loan: bool
    ) -> Result<()> 
    {
        let price_validator = &ctx.accounts.price_validator;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;
        
        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(lending_user_account.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserveOrLendingUser);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();

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
        let eighty_percent_of_deposited_usd_value = (lending_user_account.total_deposited_usd_value * 80) / 100;
        
        //Check if lending user account is in a liquidatable state
        if lending_user_account.total_borrowed_usd_value >= eighty_percent_of_deposited_usd_value
        {
            //Multiply before dividing to help keep precision
            let ten_percent_of_borrowed_amount = (lending_user_tab_account.borrowed_amount * 10) / 100;

            //You must repay atleast 10% of the borrow position if the account is in an unhealthy state. This prevents "griefing".
            //IE: Only repaying $1 (or just the smallest enough amount to be in a healthy state), front running the liquidator so their transaction fails and holding the protocol's solvency hostage!
            require!(repayment_amount >= ten_percent_of_borrowed_amount, LendingError::GriefingRepayment);
        }

        //You can't repay an amount that is greater than your borrowed amount.
        require!(lending_user_tab_account.borrowed_amount >= repayment_amount, LendingError::TooManyFunds);

        //Repay debt
        let user_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.user_ata.to_account_info().data.borrow()[..])?;
        let should_close = user_ata_data.amount == 0;
        deposit_tokens_into_token_reserve_from_user(
            ctx.accounts.token_mint.key(),
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.user_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            repayment_amount,
            should_close
        )?;

        ////////////////////////////
        //Validate Oracle Price Data
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
            temp_price_account_serialized,
            ctx.accounts.signer.key())?;

        check_token_price_staleness(temp_price_account.slot, clock_slot)?;

        let normalized_price_18_decimals = get_verified_token_price(&temp_price_account.data, token_reserve.token_id)?;

        let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 

        //Calculate the USD value of the repayment first
        let repayment_usd_value = (repayment_amount as u128 * normalized_price_18_decimals) / token_conversion_number;

        //Use saturating_sub to safely deduct the value
        //If lending_user_account.total_borrowed_usd_value falls to zero here, it just allows the user to withdraw without having to check their user health before hand. Otherwise this would get set to zero when calling withdraw again when the borrowed amounts are zero just incase this check fails.
        lending_user_account.total_borrowed_usd_value = lending_user_account
            .total_borrowed_usd_value
            .saturating_sub(repayment_usd_value);

        //Refund Oracle price account fees back to Oracle
        let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
        refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);

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
  
        msg!("{} repaid debt at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
        sub_market_owner_address.key(),
        sub_market_index);
        
        Ok(())
    }

    pub fn liquidate_account<'info>(ctx: Context<'info, LiquidateAccount<'info>>,
        repayment_sub_market_index: u16,
        liquidation_sub_market_index: u16,
        liquidati_account_index: u8,
        liquidator_account_index: u8,
        amount_to_repay: u64,
        repay_max: bool,
        paying_off_insolvent_account: bool,
        send_reward_to_wallet: bool,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()>
    {
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        let repayment_sub_market_owner_address = ctx.accounts.repayment_sub_market_owner.key();
        let liquidation_sub_market_owner_address = ctx.accounts.liquidation_sub_market_owner.key();
        let liquidati_account_owner_address = ctx.accounts.liquidati_account_owner.key();
        let clock_slot = Clock::get()?.slot;

        /////////////////////////////////
        ////Validate Liquidati Lending User Account Account
        let liquidati_lending_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_lending_account = validate_and_return_lending_user_account(*ctx.program_id,
            liquidati_lending_account_serialized,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        msg!("clock_slot: {}", clock_slot);
        msg!("liquidati_lending_account.last_health_update_clock_slot: {}", liquidati_lending_account.last_health_update_clock_slot);
        msg!("slot diff: {}", clock_slot.saturating_sub(liquidati_lending_account.last_health_update_clock_slot));
        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        #[cfg(feature = "local")] 
        require!(clock_slot.saturating_sub(liquidati_lending_account.last_health_update_clock_slot) <= 1, LendingError::StaleTokenReserveOrLendingUser);
        #[cfg(feature = "dev")]//I will set this back to 0 once deploys are working normal on solana and I'm able to use the Jito bundles on Test net
        require!(clock_slot.saturating_sub(liquidati_lending_account.last_health_update_clock_slot) <= 1000, LendingError::StaleTokenReserveOrLendingUser);

        let lending_protocol = &ctx.accounts.lending_protocol;
        let repayment_token_reserve = &mut ctx.accounts.repayment_token_reserve;
        let liquidation_token_reserve = &mut ctx.accounts.liquidation_token_reserve;
        let liquidator_lending_account = &mut ctx.accounts.liquidator_lending_account;
        let liquidator_repayment_tab_account = &mut ctx.accounts.liquidator_repayment_tab_account;
        let liquidator_liquidation_tab_account = &mut ctx.accounts.liquidator_liquidation_tab_account;
        let liquidator_repayment_monthly_statement_account = &mut ctx.accounts.liquidator_repayment_monthly_statement_account;
        let liquidator_liquidation_monthly_statement_account = &mut ctx.accounts.liquidator_liquidation_monthly_statement_account;

        //Validate remaining accounts
        let repayment_token_reserve_ata_info = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let liquidation_token_reserve_ata_info = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;

        //Repayment Token Reserve ATA
        validate_token_reserve_ata(
            repayment_token_reserve_ata_info,
            ctx.accounts.repayment_mint.key(),
            repayment_token_reserve.key()
        )?;

        //Token Reserve Liquidation ATA
        validate_token_reserve_ata(
            liquidation_token_reserve_ata_info,
            ctx.accounts.liquidation_mint.key(),
            liquidation_token_reserve.key()
        )?;

        ////////////////////////
        //Oracle Price Validator
        let price_validator_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let price_validator = validate_and_return_price_validator_account(*ctx.program_id, price_validator_serialized)?;

        ///////////////////
        //Oracle Price Data
        let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
            temp_price_account_serialized,
            ctx.accounts.signer.key())?;

        ///////////////
        //Lending Stats
        let lending_stats_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut lending_stats = validate_and_return_lending_stats_account(*ctx.program_id, lending_stats_serialized)?;

        /////////////////////////////
        //Repayment SubMarket Account
        let repayment_sub_market_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut repayment_sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            repayment_sub_market_account_serialized,
            repayment_token_reserve.token_id,
            repayment_sub_market_owner_address,
            repayment_sub_market_index)?;

        ///////////////////////////////
        //Liquidation SubMarket Account
        let liquidation_sub_market_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidation_sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            liquidation_sub_market_account_serialized,
            liquidation_token_reserve.token_id,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index)?;

        /////////////////////////////////
        //Liquidati Repayment Tab Account
        let liquidati_repayment_tab_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_repayment_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_repayment_tab_account_serialized,
            repayment_token_reserve.token_id,
            repayment_sub_market_owner_address,
            repayment_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////
        //Liquidati Liquidation Tab Account
        let liquidati_liquidation_tab_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_liquidation_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_liquidation_tab_account_serialized,
            liquidation_token_reserve.token_id,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Repayment Monthly Statement Account
        let liquidati_repayment_monthly_statement_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_repayment_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_repayment_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            repayment_token_reserve.token_id,
            repayment_sub_market_owner_address,
            repayment_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Liquidation Monthly Statement Account
        let liquidati_liquidation_monthly_statement_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_liquidation_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_liquidation_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            liquidation_token_reserve.token_id,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        let repayment_amount;
        check_token_price_staleness(temp_price_account.slot, clock_slot)?;

        //Get USD value of Repayment Amount
        let repayment_token_conversion_number = BASE_10_INT.pow(repayment_token_reserve.token_decimal_amount as u32); 
        let repayment_token_usd_value = get_verified_token_price(&temp_price_account.data, repayment_token_reserve.token_id)?;
        let mut repayment_amount_usd_value = 0;

        //Check if Account is liquidatable and set repayment_amount
        if paying_off_insolvent_account
        {
            //You can't zero out an account whose borrow liabilities aren't 100% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= liquidati_lending_account.total_deposited_usd_value, LendingError::NotInsolvent);

            if repay_max
            {
                repayment_amount = liquidati_repayment_tab_account.borrowed_amount;
                repayment_amount_usd_value = (repayment_amount as u128 * repayment_token_usd_value) / repayment_token_conversion_number;
                
                //Since all of this borrowed amount is being repaid, check if liquidati's total borrowed value would go to zero for cheaper withdrawals for them
                //Use saturating_sub to safely deduct the value
                //If lending_user_account.total_borrowed_usd_value falls to zero here, it just allows the user to withdraw without having to check their user health before hand. Otherwise this would get set to zero when calling withdraw again when the borrowed amounts are zero just incase this check fails.
                liquidati_lending_account.total_borrowed_usd_value = liquidati_lending_account
                    .total_borrowed_usd_value
                    .saturating_sub(repayment_amount_usd_value);
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
            let eighty_percent_of_liquidati_deposited_usd_value = (liquidati_lending_account.total_deposited_usd_value * 80) / 100;

            //You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= eighty_percent_of_liquidati_deposited_usd_value, LendingError::NotLiquidatable);

            //Multiply before dividing to help keep precision
            let fifty_percent_of_liquidati_borrowed_amount = (liquidati_repayment_tab_account.borrowed_amount * 50) / 100;

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

        if repayment_amount_usd_value == 0
        {
            repayment_amount_usd_value = (repayment_amount as u128 * repayment_token_usd_value) / repayment_token_conversion_number;
        }

        //Multiply before dividing to help keep precision
        let ten_percent_of_borrowed_amount = (liquidati_repayment_tab_account.borrowed_amount * 10) / 100;

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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                liquidator_lending_account,
                ctx.bumps.liquidator_lending_account,
                ctx.accounts.signer.key(),
                liquidator_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if liquidator_repayment_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_repayment_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_repayment_tab_account,
                repayment_token_reserve.token_id,
                repayment_sub_market_owner_address.key(),
                repayment_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index
            )?;
        }
        if liquidator_liquidation_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_liquidation_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_liquidation_tab_account,
                liquidation_token_reserve.token_id,
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
                repayment_token_reserve.token_id,
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
                liquidation_token_reserve.token_id,
                liquidation_sub_market_owner_address,
                liquidation_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index,
            )?;
        }

        //Update interest earned and accrued for the liquidator
        update_user_previous_interest_earned(
            repayment_token_reserve,
            &mut repayment_sub_market,
            liquidator_repayment_tab_account,
            liquidator_repayment_monthly_statement_account
        )?;
        update_user_previous_interest_accrued(
            repayment_token_reserve,
            &mut repayment_sub_market,
            liquidator_repayment_tab_account,
            liquidator_repayment_monthly_statement_account
        )?;
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
        let user_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.liquidator_repayment_ata.to_account_info().data.borrow()[..])?;
        let should_close = user_ata_data.amount == 0;
        deposit_tokens_into_token_reserve_from_user(
            ctx.accounts.repayment_mint.key(),
            &repayment_token_reserve_ata_info,
            &ctx.accounts.liquidator_repayment_ata.to_account_info(),
            &ctx.accounts.repayment_mint,
            &ctx.accounts.repayment_token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            repayment_amount,
            should_close
        )?;

        //Get USD value of Liquidation Token
        let liquidation_token_conversion_number = BASE_10_INT.pow(liquidation_token_reserve.token_decimal_amount as u32); 
        let liquidation_token_usd_value = get_verified_token_price(&temp_price_account.data, liquidation_token_reserve.token_id)?;

        let amount_to_be_liquidated = ((repayment_amount_usd_value * liquidation_token_conversion_number) / liquidation_token_usd_value) as u64;

        //Liquidate part of the Liquidati's Collateral and Transfer it plus a 7% bonus to the Liquidator
        //Multiply before dividing to help keep precision
        let mut liquidation_amount_with_7_percent_bonus = (amount_to_be_liquidated * 107) / 100;

        //Take a 1% liquidation fee
        let mut liquidation_fee_amount = amount_to_be_liquidated / 100;

        //Check for underflow if liquidation isn't profitable
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

        //Update Liquidation and Fee Values
        liquidation_sub_market.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        liquidation_sub_market.liquidated_amount += liquidation_fee_amount as u128;
        liquidation_sub_market.deposited_amount -= liquidation_fee_amount as u128;
        liquidation_sub_market.liquidation_fees_generated_amount += liquidation_fee_amount as u128;
        liquidation_token_reserve.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        liquidation_token_reserve.liquidated_amount += liquidation_fee_amount as u128;
        liquidation_token_reserve.deposited_amount -= liquidation_fee_amount as u128;
        liquidation_token_reserve.uncollected_liquidation_fees_amount += liquidation_fee_amount as u128;
        liquidati_liquidation_tab_account.deposited_amount -= liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_tab_account.deposited_amount -= liquidation_fee_amount;
        liquidati_liquidation_tab_account.liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_tab_account.liquidated_amount += liquidation_fee_amount;
        liquidator_liquidation_tab_account.liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_liquidation_tab_account.fees_generated_amount += liquidation_fee_amount;
        liquidati_liquidation_monthly_statement_account.monthly_liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_monthly_statement_account.monthly_liquidated_amount += liquidation_fee_amount;
        liquidati_liquidation_monthly_statement_account.snap_shot_balance_amount = liquidati_liquidation_tab_account.deposited_amount;
        liquidator_liquidation_monthly_statement_account.monthly_liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_liquidation_monthly_statement_account.fees_generated_amount += liquidation_fee_amount;

        if send_reward_to_wallet
        {
            let user_token_data = TokenAccount::try_deserialize(&mut &ctx.accounts.liquidator_liquidation_ata.to_account_info().data.borrow()[..])?;
            let balance_after_withdrawal = user_token_data.amount.saturating_sub(liquidation_amount_with_7_percent_bonus);
            let should_close = balance_after_withdrawal == 0;
            withdraw_tokens_from_token_reserve_to_user(
                ctx.accounts.liquidation_mint.key(),
                liquidation_token_reserve,
                &liquidation_token_reserve_ata_info.clone(),
                &ctx.accounts.liquidator_liquidation_ata.to_account_info(),
                &ctx.accounts.liquidation_mint,
                &ctx.accounts.liquidation_token_program,
                &ctx.accounts.signer,
                &ctx.accounts.system_program,
                liquidation_amount_with_7_percent_bonus,
                should_close
            )?;

            liquidation_sub_market.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128;
            liquidation_token_reserve.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128;
        }
        else
        {
            liquidator_liquidation_tab_account.deposited_amount += liquidation_amount_with_7_percent_bonus;
            liquidator_liquidation_monthly_statement_account.snap_shot_balance_amount = liquidator_liquidation_tab_account.deposited_amount;
        }

        //Refund Oracle price account fees back to Oracle
        let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
        refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);
        
        //Update Stat Listener
        lending_stats.liquidations += 1;
        
        //Update Repayment Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(repayment_token_reserve)?;
        repayment_sub_market.supply_interest_change_index = repayment_token_reserve.supply_interest_change_index;
        repayment_sub_market.borrow_interest_change_index = repayment_token_reserve.borrow_interest_change_index;
        liquidati_repayment_tab_account.supply_interest_change_index = repayment_token_reserve.supply_interest_change_index;
        liquidati_repayment_tab_account.borrow_interest_change_index = repayment_token_reserve.borrow_interest_change_index;
        liquidator_repayment_tab_account.supply_interest_change_index = repayment_token_reserve.supply_interest_change_index;
        liquidator_repayment_tab_account.borrow_interest_change_index = repayment_token_reserve.borrow_interest_change_index;

        //Update Liquidation Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
        update_token_reserve_rates(liquidation_token_reserve)?;
        liquidation_sub_market.supply_interest_change_index = liquidation_token_reserve.supply_interest_change_index;
        liquidation_sub_market.borrow_interest_change_index = liquidation_token_reserve.borrow_interest_change_index;
        liquidati_liquidation_tab_account.supply_interest_change_index = liquidation_token_reserve.supply_interest_change_index;
        liquidati_liquidation_tab_account.borrow_interest_change_index = liquidation_token_reserve.borrow_interest_change_index;
        liquidator_liquidation_tab_account.supply_interest_change_index = liquidation_token_reserve.supply_interest_change_index;
        liquidator_liquidation_tab_account.borrow_interest_change_index = liquidation_token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        let liquidation_amount = liquidation_amount_with_7_percent_bonus + liquidation_fee_amount;
        repayment_token_reserve.last_lending_activity_amount = repayment_amount;
        repayment_token_reserve.last_lending_activity_type = Activity::Repay as u8;
        liquidation_token_reserve.last_lending_activity_amount = liquidation_amount;
        liquidation_token_reserve.last_lending_activity_type = Activity::Liquidate as u8;
        repayment_sub_market.last_lending_activity_amount = repayment_amount;
        repayment_sub_market.last_lending_activity_type = Activity::Repay as u8;
        repayment_sub_market.last_lending_activity_time_stamp = repayment_token_reserve.last_lending_activity_time_stamp;
        liquidation_sub_market.last_lending_activity_amount = liquidation_amount;
        liquidation_sub_market.last_lending_activity_type = Activity::Liquidate as u8;
        liquidation_sub_market.last_lending_activity_time_stamp = liquidation_token_reserve.last_lending_activity_time_stamp;
        liquidati_repayment_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidati_repayment_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        liquidati_repayment_monthly_statement_account.last_lending_activity_time_stamp = repayment_token_reserve.last_lending_activity_time_stamp;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_time_stamp = liquidation_token_reserve.last_lending_activity_time_stamp;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_time_stamp = liquidation_token_reserve.last_lending_activity_time_stamp;
        
        //Save changes to passed in remaining accounts
        lending_stats.serialize(&mut &mut lending_stats_serialized.data.borrow_mut()[8..])?;
        repayment_sub_market.serialize(&mut &mut repayment_sub_market_account_serialized.data.borrow_mut()[8..])?;
        liquidation_sub_market.serialize(&mut &mut liquidation_sub_market_account_serialized.data.borrow_mut()[8..])?;
        liquidati_repayment_tab_account.serialize(&mut &mut liquidati_repayment_tab_account_serialized.data.borrow_mut()[8..])?;
        liquidati_liquidation_tab_account.serialize(&mut &mut liquidati_liquidation_tab_account_serialized.data.borrow_mut()[8..])?;
        liquidati_repayment_monthly_statement_account.serialize(&mut &mut liquidati_repayment_monthly_statement_account_serialized.data.borrow_mut()[8..])?;
        liquidati_liquidation_monthly_statement_account.serialize(&mut &mut liquidati_liquidation_monthly_statement_account_serialized.data.borrow_mut()[8..])?;
        
        msg!("{} liquidated {}", ctx.accounts.signer.key(), liquidati_account_owner_address.key());

        msg!("Repaid debt at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        repayment_token_reserve.token_id,
        repayment_sub_market_owner_address.key(),
        repayment_sub_market_index);

        msg!("Liquidated collateral at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        liquidation_token_reserve.token_id,
        liquidation_sub_market_owner_address.key(),
        liquidation_sub_market_index);

        Ok(())
    }

    //This liquidation is for when the repayment and liquidation tokens are the same
    pub fn liquidate_account_same_token(ctx: Context<LiquidateAccountSameToken>,
        repayment_sub_market_index: u16,
        liquidation_sub_market_index: u16,
        liquidati_account_index: u8,
        liquidator_account_index: u8,
        amount_to_repay: u64,
        repay_max: bool,
        paying_off_insolvent_account: bool,
        send_reward_to_wallet: bool,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()>
    {
        let lending_protocol = &ctx.accounts.lending_protocol;
        let price_validator = &ctx.accounts.price_validator;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let liquidati_lending_account = &mut ctx.accounts.liquidati_lending_account;
        let liquidator_lending_account = &mut ctx.accounts.liquidator_lending_account;
        let liquidator_repayment_tab_account = &mut ctx.accounts.liquidator_repayment_tab_account;
        let liquidator_liquidation_tab_account = &mut ctx.accounts.liquidator_liquidation_tab_account;
        let liquidator_repayment_monthly_statement_account = &mut ctx.accounts.liquidator_repayment_monthly_statement_account;
        let liquidator_liquidation_monthly_statement_account = &mut ctx.accounts.liquidator_liquidation_monthly_statement_account;
        let clock_slot = Clock::get()?.slot;

        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(liquidati_lending_account.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserveOrLendingUser);

        let repayment_sub_market_owner_address = ctx.accounts.repayment_sub_market_owner.key();
        let liquidation_sub_market_owner_address = ctx.accounts.liquidation_sub_market_owner.key();
        let liquidati_account_owner_address = ctx.accounts.liquidati_account_owner.key();

        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        //Validate Accounts

        ////////////////////////////
        //Oracle Price Data
        let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
            temp_price_account_serialized,
            ctx.accounts.signer.key())?;

        ///////////////
        //Lending Stats
        let lending_stats_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut lending_stats = validate_and_return_lending_stats_account(*ctx.program_id, lending_stats_serialized)?;

        /////////////////////////////
        //Repayment SubMarket Account
        let repayment_sub_market_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut repayment_sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            repayment_sub_market_account_serialized,
            token_reserve.token_id,
            repayment_sub_market_owner_address,
            repayment_sub_market_index)?;

        ///////////////////////////////
        //Liquidation SubMarket Account
        let liquidation_sub_market_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidation_sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            liquidation_sub_market_account_serialized,
            token_reserve.token_id,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index)?;

        /////////////////////////////////
        //Liquidati Repayment Tab Account
        let liquidati_repayment_tab_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_repayment_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_repayment_tab_account_serialized,
            token_reserve.token_id,
            repayment_sub_market_owner_address,
            repayment_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////
        //Liquidati Liquidation Tab Account
        let liquidati_liquidation_tab_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_liquidation_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_liquidation_tab_account_serialized,
            token_reserve.token_id,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Repayment Monthly Statement Account
        let liquidati_repayment_monthly_statement_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_repayment_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_repayment_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            token_reserve.token_id,
            repayment_sub_market_owner_address,
            repayment_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Liquidation Monthly Statement Account
        let liquidati_liquidation_monthly_statement_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_liquidation_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_liquidation_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            token_reserve.token_id,
            liquidation_sub_market_owner_address,
            liquidation_sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        let repayment_amount;
        check_token_price_staleness(temp_price_account.slot, clock_slot)?;

        //Get USD value of Repayment Amount
        let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 
        let token_usd_value = get_verified_token_price(&temp_price_account.data, token_reserve.token_id)?;
        let mut repayment_amount_usd_value = 0;

        //Check if Account is liquidatable and set repayment_amount
        if paying_off_insolvent_account
        {
            //You can't zero out an account whose borrow liabilities aren't 100% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= liquidati_lending_account.total_deposited_usd_value, LendingError::NotInsolvent);

            if repay_max
            {
                repayment_amount = liquidati_repayment_tab_account.borrowed_amount;
                repayment_amount_usd_value = (repayment_amount as u128 * token_usd_value) / token_conversion_number;
                
                //Since all of this borrowed amount is being repaid, check if liquidati's total borrowed value would go to zero for cheaper withdrawals for them
                //Use saturating_sub to safely deduct the value
                //If lending_user_account.total_borrowed_usd_value falls to zero here, it just allows the user to withdraw without having to check their user health before hand. Otherwise this would get set to zero when calling withdraw again when the borrowed amounts are zero just incase this check fails.
                liquidati_lending_account.total_borrowed_usd_value = liquidati_lending_account
                    .total_borrowed_usd_value
                    .saturating_sub(repayment_amount_usd_value);
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
            let eighty_percent_of_liquidati_deposited_usd_value = (liquidati_lending_account.total_deposited_usd_value * 80) / 100;

            //You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= eighty_percent_of_liquidati_deposited_usd_value, LendingError::NotLiquidatable);

            //Multiply before dividing to help keep precision
            let fifty_percent_of_liquidati_borrowed_amount = (liquidati_repayment_tab_account.borrowed_amount * 50) / 100;

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

        if repayment_amount_usd_value == 0
        {
            repayment_amount_usd_value = (repayment_amount as u128 * token_usd_value) / token_conversion_number;
        }

        //Multiply before dividing to help keep precision
        let ten_percent_of_borrowed_amount = (liquidati_repayment_tab_account.borrowed_amount * 10) / 100;

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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                liquidator_lending_account,
                ctx.bumps.liquidator_lending_account,
                ctx.accounts.signer.key(),
                liquidator_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if liquidator_repayment_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_repayment_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_repayment_tab_account,
                token_reserve.token_id,
                repayment_sub_market_owner_address.key(),
                repayment_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index
            )?;
        }
        if liquidator_liquidation_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_liquidation_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_liquidation_tab_account,
                token_reserve.token_id,
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
                token_reserve.token_id,
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
                token_reserve.token_id,
                liquidation_sub_market_owner_address,
                liquidation_sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index,
            )?;
        }

        //Update interest earned and accrued for the liquidator
        update_user_previous_interest_earned(
            token_reserve,
            &mut repayment_sub_market,
            liquidator_repayment_tab_account,
            liquidator_repayment_monthly_statement_account
        )?;
        update_user_previous_interest_accrued(
            token_reserve,
            &mut repayment_sub_market,
            liquidator_repayment_tab_account,
            liquidator_repayment_monthly_statement_account
        )?;
        update_user_previous_interest_earned(
            token_reserve,
            &mut liquidation_sub_market,
            liquidator_liquidation_tab_account,
            liquidator_liquidation_monthly_statement_account
        )?;
        update_user_previous_interest_accrued(
            token_reserve,
            &mut liquidation_sub_market,
            liquidator_liquidation_tab_account,
            liquidator_liquidation_monthly_statement_account
        )?;

        //Repay Liquidati's Debt
        let user_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.liquidator_ata.to_account_info().data.borrow()[..])?;
        let should_close = user_ata_data.amount == 0;
        deposit_tokens_into_token_reserve_from_user(
            ctx.accounts.token_mint.key(),
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.liquidator_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            repayment_amount,
            should_close
        )?;

        //Get Amount to be Liquidated
        let amount_to_be_liquidated = ((repayment_amount_usd_value * token_conversion_number) / token_usd_value) as u64;

        //Liquidate part of the Liquidati's Collateral and Transfer it plus a 7% bonus to the Liquidator
        //Multiply before dividing to help keep precision
        let mut liquidation_amount_with_7_percent_bonus = (amount_to_be_liquidated * 107) / 100;

        //Take a 1% liquidation fee
        let mut liquidation_fee_amount = amount_to_be_liquidated / 100;

        //Check for underflow if liquidation isn't profitable
        if liquidati_liquidation_tab_account.deposited_amount < liquidation_amount_with_7_percent_bonus + liquidation_fee_amount
        {
            //Take a 1% liquidation fee
            liquidation_fee_amount = liquidati_liquidation_tab_account.deposited_amount / 100;
            //Give remainder to liquidator
            liquidation_amount_with_7_percent_bonus = liquidati_liquidation_tab_account.deposited_amount - liquidation_fee_amount;
        }

        //Update Repayment Values
        token_reserve.borrowed_amount -= repayment_amount as u128;
        token_reserve.repaid_debt_amount += repayment_amount as u128;
        repayment_sub_market.borrowed_amount -= repayment_amount as u128;
        repayment_sub_market.repaid_debt_amount += repayment_amount as u128;
        liquidati_repayment_tab_account.borrowed_amount -= repayment_amount;
        liquidator_repayment_tab_account.repaid_debt_amount += repayment_amount;
        liquidator_repayment_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount;
        liquidati_repayment_monthly_statement_account.snap_shot_debt_amount = liquidati_repayment_tab_account.borrowed_amount;

        //Update Liquidation and Fee Values
        token_reserve.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        token_reserve.liquidated_amount += liquidation_fee_amount as u128;
        token_reserve.deposited_amount -= liquidation_fee_amount as u128;
        token_reserve.uncollected_liquidation_fees_amount += liquidation_fee_amount as u128;
        liquidation_sub_market.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        liquidation_sub_market.liquidated_amount += liquidation_fee_amount as u128;
        liquidation_sub_market.deposited_amount -= liquidation_fee_amount as u128;
        liquidation_sub_market.liquidation_fees_generated_amount += liquidation_fee_amount as u128;
        liquidati_liquidation_tab_account.deposited_amount -= liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_tab_account.deposited_amount -= liquidation_fee_amount;
        liquidati_liquidation_tab_account.liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_tab_account.liquidated_amount += liquidation_fee_amount;
        liquidator_liquidation_tab_account.liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_liquidation_tab_account.fees_generated_amount += liquidation_fee_amount;
        liquidati_liquidation_monthly_statement_account.monthly_liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_liquidation_monthly_statement_account.monthly_liquidated_amount += liquidation_fee_amount;
        liquidati_liquidation_monthly_statement_account.snap_shot_balance_amount = liquidati_liquidation_tab_account.deposited_amount;
        liquidator_liquidation_monthly_statement_account.monthly_liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_liquidation_monthly_statement_account.fees_generated_amount += liquidation_fee_amount;

        if send_reward_to_wallet
        {
            let user_token_data = TokenAccount::try_deserialize(&mut &ctx.accounts.liquidator_ata.to_account_info().data.borrow()[..])?;
            let balance_after_withdrawal = user_token_data.amount.saturating_sub(liquidation_amount_with_7_percent_bonus);
            let should_close = balance_after_withdrawal == 0;
            withdraw_tokens_from_token_reserve_to_user(
                ctx.accounts.token_mint.key(),
                token_reserve,
                &ctx.accounts.token_reserve_ata.to_account_info(),
                &ctx.accounts.liquidator_ata.to_account_info(),
                &ctx.accounts.token_mint,
                &ctx.accounts.token_program,
                &ctx.accounts.signer,
                &ctx.accounts.system_program,
                liquidation_amount_with_7_percent_bonus,
                should_close
            )?;

            token_reserve.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128;
            liquidation_sub_market.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128; 
        }
        else
        {
            liquidator_liquidation_tab_account.deposited_amount += liquidation_amount_with_7_percent_bonus;
            liquidator_liquidation_monthly_statement_account.snap_shot_balance_amount = liquidator_liquidation_tab_account.deposited_amount;
        }

        //Refund Oracle price account fees back to Oracle
        let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
        refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);
        
        //Update Stat Listener
        lending_stats.liquidations += 1;
        
        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY
        update_token_reserve_rates(token_reserve)?;

        //Update Repayment SubMarket/User time stamp based interest indexes
        repayment_sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        repayment_sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        liquidati_repayment_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidati_repayment_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        liquidator_repayment_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidator_repayment_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update Liquidation SubMarket/User time stamp based interest indexes
        liquidation_sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidation_sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        liquidati_liquidation_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidati_liquidation_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        liquidator_liquidation_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidator_liquidation_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        let liquidation_amount = liquidation_amount_with_7_percent_bonus + liquidation_fee_amount;
        //token_reserve.last_lending_activity_amount = repayment_amount;
        //token_reserve.last_lending_activity_type = Activity::Repay as u8; //Since the token is the same, make Liquidate the last activity on the token reserve
        token_reserve.last_lending_activity_amount = liquidation_amount;
        token_reserve.last_lending_activity_type = Activity::Liquidate as u8; //We'll let the Liquidate activity be the last activity since the repayment and liquidation token reserves are the same in this case
        repayment_sub_market.last_lending_activity_amount = repayment_amount;
        repayment_sub_market.last_lending_activity_type = Activity::Repay as u8;
        repayment_sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        liquidation_sub_market.last_lending_activity_amount = liquidation_amount;
        liquidation_sub_market.last_lending_activity_type = Activity::Liquidate as u8;
        liquidation_sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        liquidati_repayment_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidati_repayment_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        liquidati_repayment_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidati_liquidation_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidator_liquidation_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        
        //Save changes to passed in remaining accounts
        lending_stats.serialize(&mut &mut lending_stats_serialized.data.borrow_mut()[8..])?;
        repayment_sub_market.serialize(&mut &mut repayment_sub_market_account_serialized.data.borrow_mut()[8..])?;
        liquidation_sub_market.serialize(&mut &mut liquidation_sub_market_account_serialized.data.borrow_mut()[8..])?;
        liquidati_repayment_tab_account.serialize(&mut &mut liquidati_repayment_tab_account_serialized.data.borrow_mut()[8..])?;
        liquidati_liquidation_tab_account.serialize(&mut &mut liquidati_liquidation_tab_account_serialized.data.borrow_mut()[8..])?;
        liquidati_repayment_monthly_statement_account.serialize(&mut &mut liquidati_repayment_monthly_statement_account_serialized.data.borrow_mut()[8..])?;
        liquidati_liquidation_monthly_statement_account.serialize(&mut &mut liquidati_liquidation_monthly_statement_account_serialized.data.borrow_mut()[8..])?;
        
        msg!("{} liquidated {}", ctx.accounts.signer.key(), liquidati_account_owner_address.key());

        msg!("Repaid debt at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        token_reserve.token_id,
        repayment_sub_market_owner_address.key(),
        repayment_sub_market_index);

        msg!("Liquidated collateral at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        token_reserve.token_id,
        liquidation_sub_market_owner_address.key(),
        liquidation_sub_market_index);

        Ok(())
    }

    
    //This liquidation is for when the repayment and liquidation Sub Markets are the same. If the Sub Markets are the same, the tokens are also the same
    //The only cases not covered is liquidating yourself. You can "liquidate yourself" still, but you have to do it with a 2nd account from the same wallet
    pub fn liquidate_account_same_sub_market(ctx: Context<LiquidateAccountSameSubMarket>,
        sub_market_index: u16,
        liquidati_account_index: u8,
        liquidator_account_index: u8,
        amount_to_repay: u64,
        repay_max: bool,
        paying_off_insolvent_account: bool,
        send_reward_to_wallet: bool,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()>
    {
        let lending_protocol = &ctx.accounts.lending_protocol;
        let price_validator = &ctx.accounts.price_validator;
        let token_reserve = &mut ctx.accounts.token_reserve;
        let liquidati_lending_account = &mut ctx.accounts.liquidati_lending_account;
        let liquidator_lending_account = &mut ctx.accounts.liquidator_lending_account;
        let liquidator_tab_account = &mut ctx.accounts.liquidator_tab_account;
        let liquidator_monthly_statement_account = &mut ctx.accounts.liquidator_monthly_statement_account;

        let clock_slot = Clock::get()?.slot;

        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        require!(liquidati_lending_account.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserveOrLendingUser);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();
        let liquidati_account_owner_address = ctx.accounts.liquidati_account_owner.key();

        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        //Validate Accounts

        ////////////////////////////
        //Oracle Price Data
        let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
            temp_price_account_serialized,
            ctx.accounts.signer.key())?;

        ///////////////
        //Lending Stats
        let lending_stats_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut lending_stats = validate_and_return_lending_stats_account(*ctx.program_id, lending_stats_serialized)?;

        /////////////////////////////
        //SubMarket Account
        let sub_market_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut sub_market = validate_and_return_sub_market_account(*ctx.program_id,
            sub_market_account_serialized,
            token_reserve.token_id,
            sub_market_owner_address,
            sub_market_index)?;

        /////////////////////////////////
        //Liquidati Tab Account
        let liquidati_tab_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
            liquidati_tab_account_serialized,
            token_reserve.token_id,
            sub_market_owner_address,
            sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        ///////////////////////////////////////////////
        //Liquidati Monthly Statement Account
        let liquidati_monthly_statement_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let mut liquidati_monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
            liquidati_monthly_statement_account_serialized,
            lending_protocol.current_statement_month,
            lending_protocol.current_statement_year,
            token_reserve.token_id,
            sub_market_owner_address,
            sub_market_index,
            liquidati_account_owner_address,
            liquidati_account_index)?;

        let repayment_amount;
        check_token_price_staleness(temp_price_account.slot, clock_slot)?;

        //Get USD value of Repayment Amount
        let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 
        let token_usd_value = get_verified_token_price(&temp_price_account.data, token_reserve.token_id)?;
        let mut repayment_amount_usd_value = 0;

        //Check if Account is liquidatable and set repayment_amount
        if paying_off_insolvent_account
        {
            //You can't zero out an account whose borrow liabilities aren't 100% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= liquidati_lending_account.total_deposited_usd_value, LendingError::NotInsolvent);

            if repay_max
            {
                repayment_amount = liquidati_tab_account.borrowed_amount;
                repayment_amount_usd_value = (repayment_amount as u128 * token_usd_value) / token_conversion_number;
                
                //Since all of this borrowed amount is being repaid, check if liquidati's total borrowed value would go to zero for cheaper withdrawals for them
                //Use saturating_sub to safely deduct the value
                //If lending_user_account.total_borrowed_usd_value falls to zero here, it just allows the user to withdraw without having to check their user health before hand. Otherwise this would get set to zero when calling withdraw again when the borrowed amounts are zero just incase this check fails.
                liquidati_lending_account.total_borrowed_usd_value = liquidati_lending_account
                    .total_borrowed_usd_value
                    .saturating_sub(repayment_amount_usd_value);
            }
            else
            {
                if amount_to_repay > liquidati_tab_account.borrowed_amount
                {
                    //Can't pay more debt than the user has accumulated
                    repayment_amount = liquidati_tab_account.borrowed_amount;
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
            let eighty_percent_of_liquidati_deposited_usd_value = (liquidati_lending_account.total_deposited_usd_value * 80) / 100;

            //You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral
            require!(liquidati_lending_account.total_borrowed_usd_value >= eighty_percent_of_liquidati_deposited_usd_value, LendingError::NotLiquidatable);

            //Multiply before dividing to help keep precision
            let fifty_percent_of_liquidati_borrowed_amount = (liquidati_tab_account.borrowed_amount * 50) / 100;

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

        if repayment_amount_usd_value == 0
        {
            repayment_amount_usd_value = (repayment_amount as u128 * token_usd_value) / token_conversion_number;
        }

        //Multiply before dividing to help keep precision
        let ten_percent_of_borrowed_amount = (liquidati_tab_account.borrowed_amount * 10) / 100;

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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                liquidator_lending_account,
                ctx.bumps.liquidator_lending_account,
                ctx.accounts.signer.key(),
                liquidator_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if liquidator_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                liquidator_lending_account,
                liquidator_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_tab_account,
                token_reserve.token_id,
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index
            )?;
        }

        //Initialize monthly statement account if the statement month/year has changed or brand new sub user account.
        if liquidator_monthly_statement_account.monthly_statement_account_added == false
        {
            initialize_lending_user_monthly_statement_account(
                liquidator_monthly_statement_account,
                liquidator_tab_account,
                lending_protocol,
                ctx.bumps.liquidator_monthly_statement_account,
                token_reserve.token_id,
                sub_market_owner_address,
                sub_market_index,
                ctx.accounts.signer.key(),
                liquidator_account_index,
            )?;
        }

        //Update interest earned and accrued for the liquidator
        update_user_previous_interest_earned(
            token_reserve,
            &mut sub_market,
            liquidator_tab_account,
            liquidator_monthly_statement_account
        )?;
        update_user_previous_interest_accrued(
            token_reserve,
            &mut sub_market,
            liquidator_tab_account,
            liquidator_monthly_statement_account
        )?;

        //Repay Liquidati's Debt
        let user_ata_data = TokenAccount::try_deserialize(&mut &ctx.accounts.liquidator_ata.to_account_info().data.borrow()[..])?;
        let should_close = user_ata_data.amount == 0;
        deposit_tokens_into_token_reserve_from_user(
            ctx.accounts.token_mint.key(),
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.liquidator_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            repayment_amount,
            should_close
        )?;

        //Get Amount to be Liquidated
        let amount_to_be_liquidated = ((repayment_amount_usd_value * token_conversion_number) / token_usd_value) as u64;

        //Liquidate part of the Liquidati's Collateral and Transfer it plus a 7% bonus to the Liquidator
        //Multiply before dividing to help keep precision
        let mut liquidation_amount_with_7_percent_bonus = (amount_to_be_liquidated * 107) / 100;

        //Take a 1% liquidation fee
        let mut liquidation_fee_amount = amount_to_be_liquidated / 100;

        //Check for underflow if liquidation isn't profitable
        if liquidati_tab_account.deposited_amount < liquidation_amount_with_7_percent_bonus + liquidation_fee_amount
        {
            //Take a 1% liquidation fee
            liquidation_fee_amount = liquidati_tab_account.deposited_amount / 100;
            //Give remainder to liquidator
            liquidation_amount_with_7_percent_bonus = liquidati_tab_account.deposited_amount - liquidation_fee_amount;
        }

        //Update Repayment Values
        token_reserve.borrowed_amount -= repayment_amount as u128;
        token_reserve.repaid_debt_amount += repayment_amount as u128;
        sub_market.borrowed_amount -= repayment_amount as u128;
        sub_market.repaid_debt_amount += repayment_amount as u128;
        liquidati_tab_account.borrowed_amount -= repayment_amount;
        liquidator_tab_account.repaid_debt_amount += repayment_amount;
        liquidator_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount;
        liquidati_monthly_statement_account.snap_shot_debt_amount = liquidati_tab_account.borrowed_amount;

        //Update Liquidation and Fee Values
        token_reserve.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        token_reserve.liquidated_amount += liquidation_fee_amount as u128;
        token_reserve.deposited_amount -= liquidation_fee_amount as u128;
        token_reserve.uncollected_liquidation_fees_amount += liquidation_fee_amount as u128;
        sub_market.liquidated_amount += liquidation_amount_with_7_percent_bonus as u128;
        sub_market.liquidated_amount += liquidation_fee_amount as u128;
        sub_market.deposited_amount -= liquidation_fee_amount as u128;
        sub_market.liquidation_fees_generated_amount += liquidation_fee_amount as u128;
        liquidati_tab_account.deposited_amount -= liquidation_amount_with_7_percent_bonus;
        liquidati_tab_account.deposited_amount -= liquidation_fee_amount;
        liquidati_tab_account.liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_tab_account.liquidated_amount += liquidation_fee_amount;
        liquidator_tab_account.liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_tab_account.fees_generated_amount += liquidation_fee_amount;
        liquidati_monthly_statement_account.monthly_liquidated_amount += liquidation_amount_with_7_percent_bonus;
        liquidati_monthly_statement_account.monthly_liquidated_amount += liquidation_fee_amount;
        liquidati_monthly_statement_account.snap_shot_balance_amount = liquidati_tab_account.deposited_amount;
        liquidator_monthly_statement_account.monthly_liquidator_amount += liquidation_amount_with_7_percent_bonus;
        liquidator_monthly_statement_account.fees_generated_amount += liquidation_fee_amount;

        if send_reward_to_wallet
        {
            let user_token_data = TokenAccount::try_deserialize(&mut &ctx.accounts.liquidator_ata.to_account_info().data.borrow()[..])?;
            let balance_after_withdrawal = user_token_data.amount.saturating_sub(liquidation_amount_with_7_percent_bonus);
            let should_close = balance_after_withdrawal == 0;
            withdraw_tokens_from_token_reserve_to_user(
                ctx.accounts.token_mint.key(),
                token_reserve,
                &ctx.accounts.token_reserve_ata.to_account_info(),
                &ctx.accounts.liquidator_ata.to_account_info(),
                &ctx.accounts.token_mint,
                &ctx.accounts.token_program,
                &ctx.accounts.signer,
                &ctx.accounts.system_program,
                liquidation_amount_with_7_percent_bonus,
                should_close
            )?;

            token_reserve.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128;
            sub_market.deposited_amount -= liquidation_amount_with_7_percent_bonus as u128; 
        }
        else
        {
            liquidator_tab_account.deposited_amount += liquidation_amount_with_7_percent_bonus;
            liquidator_monthly_statement_account.snap_shot_balance_amount = liquidator_tab_account.deposited_amount;
        }

        //Refund Oracle price account fees back to Oracle
        let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
        refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);
        
        //Update Stat Listener
        lending_stats.liquidations += 1;
        
        //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY
        update_token_reserve_rates(token_reserve)?;

        //Update Repayment SubMarket/User time stamp based interest indexes
        sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
        sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        liquidati_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidati_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
        liquidator_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
        liquidator_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

        //Update last activity on accounts
        let liquidation_amount = liquidation_amount_with_7_percent_bonus + liquidation_fee_amount;
        //token_reserve.last_lending_activity_amount = repayment_amount;
        //token_reserve.last_lending_activity_type = Activity::Repay as u8; //Since the token is the same, make Liquidate the last activity on the Token Reserve
        token_reserve.last_lending_activity_amount = liquidation_amount;
        token_reserve.last_lending_activity_type = Activity::Liquidate as u8; //We'll let the Liquidate activity be the last activity since the repayment and liquidation token reserves are the same in this case
        //sub_market.last_lending_activity_amount = repayment_amount;
        //sub_market.last_lending_activity_type = Activity::Repay as u8;
        //sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp; //Since the token is the same, make Liquidate the last activity on the Sub Market
        sub_market.last_lending_activity_amount = liquidation_amount;
        sub_market.last_lending_activity_type = Activity::Liquidate as u8;
        sub_market.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        //liquidati_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        //liquidati_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        //liquidati_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;  //Since the token is the same, make Liquidate the last activity on the Monthly Statement
        liquidati_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidati_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidati_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        liquidator_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidator_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidator_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        
        //Save changes to passed in remaining accounts
        lending_stats.serialize(&mut &mut lending_stats_serialized.data.borrow_mut()[8..])?;
        sub_market.serialize(&mut &mut sub_market_account_serialized.data.borrow_mut()[8..])?;
        liquidati_tab_account.serialize(&mut &mut liquidati_tab_account_serialized.data.borrow_mut()[8..])?;
        liquidati_monthly_statement_account.serialize(&mut &mut liquidati_monthly_statement_account_serialized.data.borrow_mut()[8..])?;
        
        msg!("{} liquidated {}", ctx.accounts.signer.key(), liquidati_account_owner_address.key());

        msg!("Repaid debt and liquidated collateral at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        token_reserve.token_id,
        sub_market_owner_address.key(),
        sub_market_index);

        Ok(())
    }

    //You have to call this instruction for all user tab accounts before calling the withdraw, borrow, or liquidate functions in the same transaction.
    //It's recommended to call this refresh function on up to 5 tab sets only at a time.
    //Feed in all of the Token Reserves remaining accounts as the same order as the token_reserve_mint_addresses input, then
    //Repeating sets of these remaining accounts in this order (Up to 5 tab account sets at once, use another instruction for more): LendingUserTabAccount, Submarket, LendingUserMonthlyStatementAccount
    pub fn refresh_user_health_chunk_and_token_reserves(ctx: Context<RefreshUserHealthChunkAndTokenReserves>,
        user_account_index: u8,
        refresh_token_reserve_count: u8, //The number of token reserves being refreshed may not be the number of unverified_price_data, ie when borrowing from a token reserve the user has never interacted with before
        set_count: u8, //The number of LendingUserTabAccount, Submarket, and LendingUserMonthlyStatementAccount sets being fed in
        close_price_account: bool //When just wanting to refresh the account without borrowing, set this flag to true so it closes the price account since you don't have a burrow-like instruction that will need it.
        //Note Withdraw, Borrow, Repay, and Liquidate will close the price account when they are done.
    ) -> Result<()> 
    {
        let user_account_owner_address = ctx.accounts.lending_user_owner.key();

        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

        let lending_protocol = &ctx.accounts.lending_protocol;
        let price_validator = &ctx.accounts.price_validator;
        let lending_user_account = &mut ctx.accounts.lending_user_account;
        let time_stamp = Clock::get()?.unix_timestamp as u64;
        let clock_slot = Clock::get()?.slot;

        //Return if User Lending Account is already updated to the current block slot
        if lending_user_account.last_health_update_clock_slot == clock_slot
        {
            return Ok(())
        }

        //Check if this is an unfinished refresh or a brand new one.
        //If the block has changed since we started refreshing, we MUST reset.
        if lending_user_account.refresh_clock_slot != clock_slot
        {
            lending_user_account.temp_deposit_usd_value = 0;
            lending_user_account.temp_borrow_usd_value = 0;
            lending_user_account.next_tab_index_to_refresh = 0;
            lending_user_account.refresh_clock_slot = clock_slot;
        }

        ////////////////////////////
        //Validate Oracle Price Data
        let temp_price_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
        let temp_price_account = validate_and_return_temp_price_account(*ctx.program_id,
            temp_price_account_serialized,
            ctx.accounts.signer.key())?;

        check_token_price_staleness(temp_price_account.slot, clock_slot)?;

        let mut token_reserves: Vec<(&AccountInfo, TokenReserve)> = Vec::with_capacity(refresh_token_reserve_count.into());
        for _i in 0..refresh_token_reserve_count.into()
        {
            let token_reserve_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            let token_reserve = validate_and_return_token_reserve_account(*ctx.program_id,
                token_reserve_account_serialized)?;
            token_reserves.push((token_reserve_account_serialized, token_reserve)); 
        }

        for _i in 0..set_count.into()
        {
            //Validate Remaining Accounts

            /////////////
            //Tab Account
            let tab_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            let data_ref = tab_account_serialized.data.borrow();
            let mut data_slice: &[u8] = data_ref.deref();

            let unvalidated_lending_user_tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

            let mut lending_user_tab_account = validate_and_return_lending_user_tab_account(*ctx.program_id,
                tab_account_serialized,
                unvalidated_lending_user_tab_account.token_id,
                unvalidated_lending_user_tab_account.sub_market_owner_address,
                unvalidated_lending_user_tab_account.sub_market_index,
                user_account_owner_address,
                user_account_index)?;

            //You must provide all of the sub user's tab accounts ordered by user_tab_account_index
            require!(lending_user_account.next_tab_index_to_refresh == lending_user_tab_account.user_tab_account_index, LendingError::IncorrectOrderOfTabAccounts);
            
            drop(data_ref);

            ///////////////////////
            //Token Reserve Account
            let token_reserve_entry = token_reserves.iter_mut()
                .find(|(_, token_reserve)| token_reserve.token_id == lending_user_tab_account.token_id)
                .ok_or(LendingError::MissingTokenReserveAccountForRefresh)?;
            let (token_reserve_account_serialized, token_reserve) = token_reserve_entry;

            ///////////////////
            //SubMarket Account
            let sub_market_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            let mut sub_market = validate_and_return_sub_market_account(*ctx.program_id,
                sub_market_account_serialized,
                lending_user_tab_account.token_id,
                lending_user_tab_account.sub_market_owner_address,
                lending_user_tab_account.sub_market_index)?;

            ///////////////////////////
            //Monthly Statement Account
            let monthly_statement_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            let mut monthly_statement_account = validate_and_return_lending_user_monthly_state_account(*ctx.program_id,
                monthly_statement_account_serialized,
                lending_protocol.current_statement_month,
                lending_protocol.current_statement_year,
                lending_user_tab_account.token_id,
                lending_user_tab_account.sub_market_owner_address,
                lending_user_tab_account.sub_market_index,
                user_account_owner_address,
                user_account_index)?;

            //Calculate Token Reserve Previously Earned And Accrued Interest
            if token_reserve.last_health_update_clock_slot != clock_slot
            {
                update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, Some(clock_slot))?;
            }
            
            update_user_previous_interest_earned(
                token_reserve,
                &mut sub_market,
                &mut lending_user_tab_account,
                &mut monthly_statement_account
            )?;

            update_user_previous_interest_accrued(
                token_reserve,
                &mut sub_market,
                &mut lending_user_tab_account,
                &mut monthly_statement_account
            )?;
            
            //Update Token Reserve Global Utilization Rate, Borrow APY, Supply APY, and the SubMarket/User time stamp based interest indexes
            update_token_reserve_rates(token_reserve)?;
            sub_market.supply_interest_change_index = token_reserve.supply_interest_change_index;
            sub_market.borrow_interest_change_index = token_reserve.borrow_interest_change_index;
            lending_user_tab_account.supply_interest_change_index = token_reserve.supply_interest_change_index;
            lending_user_tab_account.borrow_interest_change_index = token_reserve.borrow_interest_change_index;

            //Get normalized price with 8 decimals
            let normalized_price_18_decimals = get_verified_token_price(&temp_price_account.data, token_reserve.token_id)?;
            
            //Update temp deposited and borrow values
            let token_conversion_number = BASE_10_INT.pow(token_reserve.token_decimal_amount as u32); 
            lending_user_account.temp_deposit_usd_value += (lending_user_tab_account.deposited_amount as u128 * normalized_price_18_decimals) / token_conversion_number;
            lending_user_account.temp_borrow_usd_value += (lending_user_tab_account.borrowed_amount as u128 * normalized_price_18_decimals) / token_conversion_number;

            lending_user_account.next_tab_index_to_refresh += 1;

            //1. Save Token Reserve (Skip 8 byte discriminator)
            token_reserve.serialize(&mut &mut token_reserve_account_serialized.data.borrow_mut()[8..])?;

            //2. Save SubMarket (Skip 8 byte discriminator)
            sub_market.serialize(&mut &mut sub_market_account_serialized.data.borrow_mut()[8..])?;

            //3. Save User Tab Account (Skip 8 byte discriminator)
            lending_user_tab_account.serialize(&mut &mut tab_account_serialized.data.borrow_mut()[8..])?;

            //4. Save Monthly Statement (Skip 8 byte discriminator)
            monthly_statement_account.serialize(&mut &mut monthly_statement_account_serialized.data.borrow_mut()[8..])?;
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

        if close_price_account
        {
            //Refund Oracle price account fees back to Oracle
            let oracle_account_serialized = remaining_accounts_iter.next().ok_or(LendingError::MissingRemainingAccount)?;
            require_keys_eq!(oracle_account_serialized.key(), price_validator.address, LendingError::PriceOracleKeyMisMatched);
            refund_oracle_temp_account_fees(temp_price_account_serialized, oracle_account_serialized);
        }

        Ok(())
    }

    pub fn create_new_monthly_statement(ctx: Context<CreateNewMonthlyStatement>, token_id: u8, sub_market_index: u16, user_account_index: u8) -> Result<()> 
    {
        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();
        let user_account_owner_address = ctx.accounts.lending_user_owner.key();
        let lending_protocol = &ctx.accounts.lending_protocol;
        let lending_user_tab_account = &ctx.accounts.lending_user_tab_account;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;

        initialize_lending_user_monthly_statement_account(
            lending_user_monthly_statement_account,
            lending_user_tab_account,
            lending_protocol,
            ctx.bumps.lending_user_monthly_statement_account,
            token_id,
            sub_market_owner_address.key(),
            sub_market_index,
            user_account_owner_address.key(),
            user_account_index,
        )?;

        Ok(())
    }

    pub fn claim_sub_market_fees(ctx: Context<ClaimSubMarketFees>,
        sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()> 
    {
        let sub_market = &mut ctx.accounts.sub_market;
        //Only the Fee Collector can call this function
        require_keys_eq!(ctx.accounts.signer.key(), sub_market.fee_collector_address.key(), LendingError::NotFeeCollector);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();
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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                lending_user_account,
                ctx.bumps.lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_tab_account,
                token_reserve.token_id, 
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
                token_reserve.token_id,
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
        lending_user_monthly_statement_account.monthly_sub_market_fees_collected_amount += sub_market.uncollected_sub_market_fees_amount as u64;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;

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

        msg!("{} Collected SubMarket Fees at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
        sub_market_owner_address.key(),
        sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

        Ok(())
    }

    pub fn claim_sub_market_fees_and_deposit_in_different_sub_market(ctx: Context<ClaimSubMarketFeesAndDepositInDifferentSubMarket>,
        initial_sub_market_index: u16,
        destination_sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()> 
    {
        let initial_sub_market_owner_address = ctx.accounts.initial_sub_market_owner.key();
        let destination_sub_market_owner_address = ctx.accounts.destination_sub_market_owner.key();
        let initial_sub_market = &mut ctx.accounts.initial_sub_market;
        //Only the Fee Collector can call this function
        require_keys_eq!(ctx.accounts.signer.key(), initial_sub_market.fee_collector_address.key(), LendingError::NotFeeCollector);
                
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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                lending_user_account,
                ctx.bumps.lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if initial_lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                initial_lending_user_tab_account,
                lending_protocol,
                ctx.bumps.initial_lending_user_tab_account,
                token_reserve.token_id,
                initial_sub_market_owner_address,
                initial_sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index
            )?;
        }
        if destination_lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                destination_lending_user_tab_account,
                lending_protocol,
                ctx.bumps.destination_lending_user_tab_account,
                token_reserve.token_id,
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
                token_reserve.token_id,
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
                token_reserve.token_id,
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
        initial_lending_user_monthly_statement_account.monthly_sub_market_fees_collected_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64;
        initial_lending_user_monthly_statement_account.monthly_withdrawal_amount += initial_sub_market.uncollected_sub_market_fees_amount as u64; //Treating this as a withdrawal from initial submarket. The fee collection and withdrawal cancel each other out, so no need to update snap shot balance for initial submarket.
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

        msg!("{} Collected SubMarket Fees at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
        initial_sub_market_owner_address.key(),
        initial_sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

        msg!("Fees Moved to DestinationSubMarketOwner: {}, DestinationSubMarketIndex: {}", destination_sub_market_owner_address.key(), destination_sub_market_index);

        Ok(())
    }

    pub fn claim_solvency_insurance_fees(ctx: Context<ClaimSolvencyInsuranceFees>,
        sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()> 
    {
        let solvency_treasurer = &ctx.accounts.solvency_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), solvency_treasurer.address.key(), LendingError::NotSolvencyTreasurer);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();
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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                lending_user_account,
                ctx.bumps.lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_tab_account,
                token_reserve.token_id,
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
                token_reserve.token_id,
                sub_market_owner_address.key(),
                sub_market_index,
                ctx.accounts.signer.key(),
                user_account_index,
            )?;
        }

        let amount = token_reserve.uncollected_solvency_insurance_fees_amount as u64;
        let user_token_data = TokenAccount::try_deserialize(&mut &ctx.accounts.treasurer_ata.to_account_info().data.borrow()[..])?;
        let balance_after_withdrawal = user_token_data.amount.saturating_sub(amount);
        let should_close = balance_after_withdrawal == 0;
        withdraw_tokens_from_token_reserve_to_user(
            ctx.accounts.token_mint.key(),
            token_reserve,
            &ctx.accounts.token_reserve_ata.to_account_info(),
            &ctx.accounts.treasurer_ata.to_account_info(),
            &ctx.accounts.token_mint,
            &ctx.accounts.token_program,
            &ctx.accounts.signer,
            &ctx.accounts.system_program,
            amount,
            should_close
        )?;

        //Record Solvency Insurance Fee Collection
        lending_user_monthly_statement_account.monthly_solvency_insurance_fees_collected_amount += amount;

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

        msg!("{} Collected Solvency Insurance Fees at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
        sub_market_owner_address.key(),
        sub_market_index);

        msg!("FeeCollectorAccountIndex: {}", user_account_index);

        Ok(())
    }

    pub fn claim_liquidation_fees(ctx: Context<ClaimLiquidationFees>,
        sub_market_index: u16,
        user_account_index: u8,
        account_name: Option<String>, //Optional variable. Use null on front end when not needed
        look_up_table_address: Option<Pubkey> //Needed when a user initializes their Lending User Account
    ) -> Result<()> 
    {
        let liquidation_treasurer = &ctx.accounts.liquidation_treasurer;
        //Only the Treasurer can call this function
        require_keys_eq!(ctx.accounts.signer.key(), liquidation_treasurer.address.key(), LendingError::NotLiquidationTreasurer);

        let sub_market_owner_address = ctx.accounts.sub_market_owner.key();
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

            let lut_address = look_up_table_address.ok_or(LendingError::MissingLendingUserLookUpTable)?;

            initialize_lending_user_account(
                lending_user_account,
                ctx.bumps.lending_user_account,
                ctx.accounts.signer.key(),
                user_account_index,
                new_account_name_to_use,
                lut_address
            )?;
        }

        //Populate tab account if being newly initialized. Every token the lending user interacts with has its own tab account tied to that sub user and their account index.
        if lending_user_tab_account.user_tab_account_added == false
        {
            let lending_protocol = &ctx.accounts.lending_protocol;
            initialize_lending_user_tab_account(
                lending_user_account,
                lending_user_tab_account,
                lending_protocol,
                ctx.bumps.lending_user_tab_account,
                token_reserve.token_id, 
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
                token_reserve.token_id,
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
        lending_user_monthly_statement_account.monthly_liquidation_fees_collected_amount += token_reserve.uncollected_liquidation_fees_amount as u64;
        lending_user_monthly_statement_account.snap_shot_balance_amount = lending_user_tab_account.deposited_amount;

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

        msg!("{} Collected Liquidation Fees at Token ID: {}, SubMarketOwner: {}, SubMarketIndex: {}",
        ctx.accounts.signer.key(),
        token_reserve.token_id,
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
    ///CHECK: This is the new address of the Lending Protocol Look Up Table Account
    pub look_up_table_address: UncheckedAccount<'info>,

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
        seeds = [b"oraclePriceValidator".as_ref()],
        bump,
        space = size_of::<OraclePriceValidator>() + 8)]
    pub price_validator: Account<'info, OraclePriceValidator>,

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
    ///CHECK: This is the address of the new Lending CEO
    pub new_ceo_address: UncheckedAccount<'info>,

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
    ///CHECK: This is the address of the new Solvency Treasurer
    pub new_treasurer_address: UncheckedAccount<'info>,

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
    ///CHECK: This is the address of the new Liquidation Treasurer
    pub new_treasurer_address: UncheckedAccount<'info>,

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
pub struct SetOraclePriceValidator<'info> 
{
    ///CHECK: This is the address of the new Oracle Price Validator
    pub new_price_validator_address: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, LendingProtocolCEO>,

    #[account(
        mut,
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, OraclePriceValidator>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(payload: PriceDataPayload)]
pub struct CreateTempOraclePriceData<'info> 
{
    ///CHECK: This is the address of the lending user requesting the price data
    pub lending_user_address: UncheckedAccount<'info>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, OraclePriceValidator>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"oraclePriceData".as_ref(), lending_user_address.key().as_ref()], 
        bump,
        space = (payload.data.len() * 17) + 1 + 4 + 8 + 8)]//Token Prices Count * (token_id(1byte) + normalized_price_18_decimals(16bytes) = 17bytes)
        //1(Bump) + 4(Borsh Vector Prefix) + 8(slot) + 8(Anchor Discriminator)
    pub temp_price_account: Account<'info, TempOraclePriceAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

//This would normally be closed by the refresh_user_health_chunk_and_token_reserves (with close set to true), withdraw, borrow, repay, or a liquidation instruction, this is just in case 
#[derive(Accounts)]
pub struct CloseTempOraclePriceData<'info> 
{
    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"oraclePriceData".as_ref(), signer.key().as_ref()], 
        bump)]
    pub temp_price_account: Account<'info, TempOraclePriceAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct UpdateLendingProtocol<'info> 
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
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump, 
        space = size_of::<TokenReserve>() + 8)]
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(
        init, 
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct UpdateTokenReserve<'info> 
{
    ///CHECK: This is the token mint address of the Token Reserve the CEO wants to update
    pub token_mint_address: UncheckedAccount<'info>,

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
#[instruction(sub_market_index: u16)]
pub struct CreateSubMarket<'info> 
{
    ///CHECK: This is the token mint address of the Token Reserve the user wants to create a Sub Market under
    pub token_mint_address: UncheckedAccount<'info>,

    ///CHECK: This is the fee collector address that the Sub Market owner wants to designate to be able to collect fees from this Sub Market
    pub fee_collector_address: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"subMarketStats".as_ref()],
        bump)]
    pub sub_market_stats: Account<'info, SubMarketStats>,

    #[account(
        init,
        payer = signer,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), signer.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<SubMarket>() + 8)]
    pub sub_market: Account<'info, SubMarket>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"subMarketOwnerLookUpTable".as_ref(), signer.key().as_ref()], 
        bump, 
        space = size_of::<SubMarketOwnerLookUpTable>() + 8)]
    pub sub_market_owner_look_up_table: Account<'info, SubMarketOwnerLookUpTable>,

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
#[instruction(token_id: u8, sub_market_index: u16)]
pub struct EditSubMarket<'info> 
{
    ///CHECK: This is the fee collector address that the Sub Market owner wants to designate to be able to collect fees from this Sub Market
    pub fee_collector_address: UncheckedAccount<'info>,
    
    #[account(
        mut,
        seeds = [b"subMarketStats".as_ref()],
        bump)]
    pub sub_market_stats: Account<'info, SubMarketStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_id.to_le_bytes().as_ref(), signer.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, SubMarket>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

/*#[derive(Accounts)]
#[instruction(user_account_index: u8)]
pub struct UpdateLendingUserLookUpTableAddress<'info> 
{
    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}*/

#[derive(Accounts)]
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct DepositTokens<'info> 
{
    ///CHECK: This is the wallet address of the user who owns the Sub Market
    pub sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, LendingStats>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be deposited as wSol and the user may or may not have a wSol account already.
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_mint: InterfaceAccount<'info, Mint>,
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
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct WithdrawTokens<'info> 
{
    ///CHECK: This is the wallet address of the user who owns the Sub Market
    pub sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, LendingStats>>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Box<Account<'info, OraclePriceValidator>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Box<Account<'info, LendingUserAccount>>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed, //Users that withdraw with no debt won't have to use the refresh_user_health_chunk instruction. Create monthly statement if it doesn't exist.
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be withdrawn as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct BorrowTokens<'info> 
{
    ///CHECK: This is the wallet address of the user who owns the Sub Market
    pub sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, LendingStats>>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Box<Account<'info, OraclePriceValidator>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Box<Account<'info, LendingUserAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //Init ATA account of token being borrowed if it doesn't exist for User
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct RepayTokens<'info> 
{
    ///CHECK: This is the wallet address of the user who owns the Sub Market
    pub sub_market_owner: UncheckedAccount<'info>,

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
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Box<Account<'info, OraclePriceValidator>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>, 

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Box<Account<'info, LendingUserAccount>>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        mut,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSol and the user may or may not have a wSol account already.
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub user_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(repayment_sub_market_index: u16,
    liquidation_sub_market_index: u16,
    liquidati_account_index: u8,
    liquidator_account_index: u8)]
pub struct LiquidateAccount<'info>
{
    ///CHECK: This is the wallet address of the liquidati (borrower) being liquidated
    pub liquidati_account_owner: UncheckedAccount<'info>,
    ///CHECK: This is the wallet address of the user who owns the repayment Sub Market
    pub repayment_sub_market_owner: UncheckedAccount<'info>,
    ///CHECK: This is the wallet address of the user who owns the liquidation Sub Market
    pub liquidation_sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), repayment_mint.key().as_ref()], 
        bump)]
    pub repayment_token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), liquidation_mint.key().as_ref()], 
        bump)]
    pub liquidation_token_reserve: Box<Account<'info, TokenReserve>>,

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
        repayment_token_reserve.token_id.to_le_bytes().as_ref(),
        repayment_sub_market_owner.key().as_ref(),
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
        liquidation_token_reserve.token_id.to_le_bytes().as_ref(),
        liquidation_sub_market_owner.key().as_ref(),
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
        repayment_token_reserve.token_id.to_le_bytes().as_ref(),
        repayment_sub_market_owner.key().as_ref(),
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
        liquidation_token_reserve.token_id.to_le_bytes().as_ref(),
        liquidation_sub_market_owner.key().as_ref(),
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

    pub repayment_mint: Box<InterfaceAccount<'info, Mint>>,
    pub liquidation_mint: Box<InterfaceAccount<'info, Mint>>,
    pub repayment_token_program: Interface<'info, TokenInterface>,
    pub liquidation_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(repayment_sub_market_index: u16,
    liquidation_sub_market_index: u16,
    liquidati_account_index: u8,
    liquidator_account_index: u8)]
pub struct LiquidateAccountSameToken<'info>
{
    ///CHECK: This is the wallet address of the liquidati (borrower) being liquidated
    pub liquidati_account_owner: UncheckedAccount<'info>,
    ///CHECK: This is the wallet address of the user who owns the repayment Sub Market
    pub repayment_sub_market_owner: UncheckedAccount<'info>,
    ///CHECK: This is the wallet address of the user who owns the liquidation Sub Market
    pub liquidation_sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), liquidati_account_owner.key().as_ref(), liquidati_account_index.to_le_bytes().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        repayment_sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        liquidation_sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        repayment_sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        liquidation_sub_market_owner.key().as_ref(),
        liquidation_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidator_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_liquidation_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub liquidator_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(sub_market_index: u16,
    liquidati_account_index: u8,
    liquidator_account_index: u8)]
pub struct LiquidateAccountSameSubMarket<'info>
{
    ///CHECK: This is the wallet address of the liquidati (borrower) being liquidated
    pub liquidati_account_owner: UncheckedAccount<'info>,
    ///CHECK: This is the wallet address of the user who owns the Sub Market
    pub sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), liquidati_account_owner.key().as_ref(), liquidati_account_index.to_le_bytes().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidati_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub liquidator_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        liquidator_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be repaid as wSOL then converted to SOL for User. This function also closes user wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub liquidator_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

//The monthly statement accounts have to exists before calling the refresh_user_health_chunk instruction.
//Use the create_new_monthly_statement function if it's a new month and it doesn't exist yet.
//This refreshes the Lending User Account and associated Token Reserves
#[derive(Accounts)]
#[instruction(user_account_index: u8)]
pub struct RefreshUserHealthChunkAndTokenReserves<'info> 
{
    ///CHECK: This is the wallet address of the Lending User having their health refreshed
    pub lending_user_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), lending_user_owner.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump)]
    pub lending_user_account: Account<'info, LendingUserAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(token_id: u8, sub_market_index: u16, user_account_index: u8)]
pub struct CreateNewMonthlyStatement<'info> 
{
    ///CHECK: This is the Sub Market Owner address for the monthly statement that will be created
    pub sub_market_owner: UncheckedAccount<'info>,
    ///CHECK: This is the Lending User wallet address for the monthly statement that will be created
    pub lending_user_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

    #[account(
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        lending_user_owner.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, LendingUserTabAccount>,

    #[account(
        init,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        lending_user_owner.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, LendingUserMonthlyStatementAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct ClaimSubMarketFees<'info> 
{
    ///CHECK: This is the Token Mint address for the Token Reserve for the Sub Market where the fees are being claimed
    pub token_mint_address: UncheckedAccount<'info>,
    ///CHECK: This is the Sub Market Owner address for the fees being claimed
    pub sub_market_owner: UncheckedAccount<'info>,

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
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
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
#[instruction(initial_sub_market_index: u16, destination_sub_market_index: u16, user_account_index: u8)]
pub struct ClaimSubMarketFeesAndDepositInDifferentSubMarket<'info> 
{
    ///CHECK: This is the Token Mint address for the Token Reserve for the Sub Market where the fees are being claimed
    pub token_mint_address: UncheckedAccount<'info>,
    ///CHECK: This is the Initial Sub Market Owner address for the fees being claimed
    pub initial_sub_market_owner: UncheckedAccount<'info>,
    ///CHECK: This is the Destination Sub Market Owner address for the fees being claimed
    pub destination_sub_market_owner: UncheckedAccount<'info>,

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
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), initial_sub_market_owner.key().as_ref(), initial_sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub initial_sub_market: Box<Account<'info, SubMarket>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), destination_sub_market_owner.key().as_ref(), destination_sub_market_index.to_le_bytes().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        initial_sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        destination_sub_market_owner.key().as_ref(),
        destination_sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub destination_lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        initial_sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        destination_sub_market_owner.key().as_ref(),
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
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct ClaimSolvencyInsuranceFees<'info> 
{
    ///CHECK: This is the Sub Market Owner address for the solvency fees being claimed
    pub sub_market_owner: UncheckedAccount<'info>,

    #[account(
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Box<Account<'info, LendingProtocol>>,

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
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Box<Account<'info, LendingUserTabAccount>>,

    //The SubMarket doesn't matter that much here since all of the fees are collected from the Token Reserve, but a SubMarket is still neccessary for using the monthly statements
    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userMonthlyStatementAccount".as_ref(),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        lending_protocol.current_statement_month.to_le_bytes().as_ref(),
        lending_protocol.current_statement_year.to_le_bytes().as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, LendingUserMonthlyStatementAccount>>,

    #[account(
        init_if_needed, //SOL has to be claimed as wSOL then converted to SOL for Treasurer. This function also closes wSOL ata if it is empty.
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program
    )]
    pub treasurer_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = token_reserve,
        associated_token::token_program = token_program
    )]
    pub token_reserve_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(sub_market_index: u16, user_account_index: u8)]
pub struct ClaimLiquidationFees<'info> 
{
    ///CHECK: This is the Token Mint address for the liquidation fees being claimed
    pub token_mint_address: UncheckedAccount<'info>,
    ///CHECK: This is the Sub Market Owner address for the liquidation fees being claimed
    pub sub_market_owner: UncheckedAccount<'info>,

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
        seeds = [b"liquidationTreasurer".as_ref()],
        bump)]
    pub liquidation_treasurer: Account<'info, LiquidationTreasurer>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
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
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
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
    pub fixed_borrow_apy: u16,
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