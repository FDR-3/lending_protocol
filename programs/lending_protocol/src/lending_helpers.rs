use anchor_lang::prelude::*;
use anchor_lang::system_program::{self};
use anchor_spl::token_interface::{self, Mint, TokenInterface, TransferChecked, SyncNative, CloseAccount};
use ra_solana_math::FixedPoint;
use crate::errors::LendingError;
use crate::structs as Structs;

const SOL_TOKEN_MINT_ADDRESS: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

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
pub fn update_token_reserve_supply_and_borrow_interest_change_index<'info>(
    token_reserve: &mut Structs::TokenReserve, 
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
pub fn update_token_reserve_rates<'info>(token_reserve: &mut Structs::TokenReserve) -> Result<()>
{
    if token_reserve.borrowed_amount == 0
    {
        token_reserve.utilization_rate = 0;
        token_reserve.supply_apy = 0; //There can be no supply apy if no one is borrowing
        token_reserve.borrow_apy = token_reserve.base_borrow_apy;
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
            token_reserve.borrow_apy = token_reserve.base_borrow_apy;
        }
        else
        {
            let optimal_utilization_rate = 7_000; //7_000 = 70.00%
            let utilization_rate = token_reserve.utilization_rate as u128;
            
            //Borrow APY = Borrow APY Base(Borrow APY Slope1 in this case) + ((Utilization Rate/Optimal Utialization Rate) * Borrow APY Slope1)
            //Setting Borrow APY Base to Borrow APY Slope1 in this case
            if utilization_rate < optimal_utilization_rate
            {
                //Max Borrow Rate = token_reserve.base_borrow_apy + token_reserve.base_borrow_apy @Less Than 70% Utilization Rate
                let borrow_apy_slope1 = token_reserve.base_borrow_apy as u128;
                //Multiply before dividing to help keep precision
                let u_rate_times_borrow_apy_slope1 = utilization_rate * borrow_apy_slope1;
                let u_rate_times_borrow_apy_slope1_divide_optimal_u_rate = u_rate_times_borrow_apy_slope1 / optimal_utilization_rate;

                //Max Borrow Rate = token_reserve.base_borrow_apy + token_reserve.base_borrow_apy @Less Than 70% Utilization Rate
                token_reserve.borrow_apy = (borrow_apy_slope1 + u_rate_times_borrow_apy_slope1_divide_optimal_u_rate) as u16;
            }
            else
            {
                //Max Borrow Rate = 10% + 34% = 44% @100% Utilization Rate. Max base borrow apy is 5%. I think having a rate more than 44% would appear too pay day loany...just seems like a bad look lol.
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

                //Max Borrow Rate = 10% + 34% = 44% @100% Utilization Rate. Max base borrow apy is 5%.
                token_reserve.borrow_apy = (token_reserve.base_borrow_apy * 2) + new_high_rate_base as u16;
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
pub fn update_user_previous_interest_earned<'info>(
    token_reserve: &mut Structs::TokenReserve,
    sub_market: &mut Structs::SubMarket,
    lending_user_tab_account: &mut Structs::LendingUserTabAccount,
    lending_user_monthly_statement_account: &mut Structs::LendingUserMonthlyStatementAccount
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
pub fn update_user_previous_interest_accrued<'info>(
    token_reserve: &mut Structs::TokenReserve,
    sub_market: &mut Structs::SubMarket,
    lending_user_tab_account: &mut Structs::LendingUserTabAccount,
    lending_user_monthly_statement_account: &mut Structs::LendingUserMonthlyStatementAccount
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

pub fn check_token_price_staleness(price_data_clock_slot: u64, current_clock_slot: u64) -> Result<()>
{
    //Allow a max age of 75 slots (approx 30 seconds)
    if current_clock_slot.saturating_sub(price_data_clock_slot) > 75 //The price data clock slot is set by the m4a api right before it sends off the bundles. There can be a slight delay by the time the bundle executes everything in the same slot, so it's not the slot that the api wrote.
    {                                                                //But the price can only come from the api and it will always fire off immediately if input is correct. This is more of a safety check, incase like the api price server got stuck and was holding on to an old price for some reason.
        msg!("Current Slot: {}", current_clock_slot);                //StaleTokenReserveOrLendingUser error checks will ensure the necessary transactions atleast execute in the same slot. 75 slots, 400ms per slot, about 30 seconds
        msg!("Data Slot: {}", price_data_clock_slot);                //Think of this as the amount of time the Jito Bundle has to find a slot to execute on
        return Err(error!(LendingError::OracleDataStale));
    }

    Ok(())
}

pub fn refund_oracle_temp_account_fees(temp_price_account_info: &AccountInfo, oracle_account_info: &AccountInfo)
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

pub fn get_verified_token_price(verified_token_prices: &[Structs::VerifiedPriceData], token_id: u8) -> Result<u128>
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

pub fn deposit_tokens_into_token_reserve_from_user<'info>(token_mint_address: Pubkey,
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

pub fn withdraw_tokens_from_token_reserve_to_user<'info>(token_mint_address: Pubkey,
    token_reserve: &Account<'info, Structs::TokenReserve>,
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