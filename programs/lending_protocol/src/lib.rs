use anchor_lang::prelude::*;
use anchor_spl::token_interface::{TokenAccount};
use solana_security_txt::security_txt;
use std::ops::Deref;
pub mod validation;
pub mod errors;
pub mod initialization;
pub mod contexts;
pub mod structs;
pub mod lending_helpers;
pub mod shared_constants;
use crate::contexts::*;
use crate::errors::LendingError;
use crate::initialization::*;
use crate::lending_helpers::*;
use crate::structs as Structs;
use crate::validation::*;
use crate::shared_constants::MAX_ACCOUNT_NAME_LENGTH;

declare_id!("LendVMybdnkGL9yX9VFJamrtCSzL3izpUoB9JDhSU6M");

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

const INITIAL_MAX_TABS_PER_LENDING_ACCOUNT: u8 = 10;
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

    pub fn create_temp_oracle_price_data(ctx: Context<CreateTempOraclePriceData>, payload: Structs::PriceDataPayload) -> Result<()> 
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
        base_borrow_apy: u16,
        use_fixed_borrow_apy: bool,
        global_limit: u128,
        solvency_insurance_fee_rate: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        //Base Borrow APY can't be greater than 5%, 0.05 in decimal form, 500 in fixed point notation
        require!(base_borrow_apy <= 500, LendingError::InvalidBaseBorrowAPY);

        //Solvency Insurance Fee on interest earned rate can't be greater than 4%, 0.04 in decimal form, 400 in fixed point notation
        require!(solvency_insurance_fee_rate <= 400, LendingError::InvalidSolvencyInsuranceFeeRate);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.bump = ctx.bumps.token_reserve;
        token_reserve.token_mint_address = ctx.accounts.token_mint.key();
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.borrow_apy = base_borrow_apy;
        token_reserve.base_borrow_apy = base_borrow_apy;
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
        msg!("Base Borrow APY: {}", base_borrow_apy);
        msg!("Use fixed Borrow APY: {}", use_fixed_borrow_apy);
        msg!("Global Limit: {}", global_limit);
            
        Ok(())
    }

    pub fn update_token_reserve(ctx: Context<UpdateTokenReserve>,
        base_borrow_apy: u16,
        use_fixed_borrow_apy: bool,
        global_limit: u128,
        solvency_insurance_fee_rate: u16) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), LendingError::NotCEO);

        //Base Borrow APY can't be greater than 5%, 0.05 in decimal form, 500 in fixed point notation
        require!(base_borrow_apy <= 500, LendingError::InvalidBaseBorrowAPY);

        //Solvency Insurance Fee on interest earned rate can't be greater than 4%, 0.04 in decimal form, 400 in fixed point notation
        require!(solvency_insurance_fee_rate <= 400, LendingError::InvalidSolvencyInsuranceFeeRate);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;

        //If the value of the Token Reserve Borrow APY will change, calculate previous interest changes before updating it
        if token_reserve.base_borrow_apy != base_borrow_apy || token_reserve.use_fixed_borrow_apy != use_fixed_borrow_apy
        {
            let time_stamp = Clock::get()?.unix_timestamp as u64;

            //Calculate Token Reserve Previously Earned And Accrued Interest
            update_token_reserve_supply_and_borrow_interest_change_index(token_reserve, time_stamp, None)?;
        }

        token_reserve.base_borrow_apy = base_borrow_apy;
        token_reserve.use_fixed_borrow_apy = use_fixed_borrow_apy;
        token_reserve.global_limit = global_limit;
        token_reserve.solvency_insurance_fee_rate = solvency_insurance_fee_rate;
        token_reserve_stats.token_reserves_updated_count += 1;

        //Update Token Reserve Global Utilization Rate, Borrow APY, and, Supply APY
        update_token_reserve_rates(token_reserve)?;

        msg!("Token Reserve Updated");
        msg!("Token ID: {}", token_reserve.token_id);
        msg!("New Base Borrow APY: {}", base_borrow_apy);
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
        pay_off_loan: bool,
        pay_10_percent: bool
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
        else if pay_10_percent
        {
            repayment_amount = (lending_user_tab_account.borrowed_amount * 10) / 100;
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

        //This function instruction must be called in the same transaction after the refresh_user_health_chunk function instruction(s)
        #[cfg(feature = "local")] 
        require!(clock_slot.saturating_sub(liquidati_lending_account.last_health_update_clock_slot) <= 1, LendingError::StaleTokenReserveOrLendingUser);
        #[cfg(feature = "dev")]
        require!(liquidati_lending_account.last_health_update_clock_slot == clock_slot, LendingError::StaleTokenReserveOrLendingUser);
        
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
        liquidati_repayment_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount; //Update liquidati monthly statement repayment amount, but not the tab. The tab is for the leader board and the liquidati shouldn't get credit for repayment in this case, but updating the monthly statement atleast for visibility to the liquidati.
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
        liquidator_liquidation_monthly_statement_account.monthly_fees_generated_amount += liquidation_fee_amount;

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
        liquidator_repayment_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidator_repayment_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        liquidator_repayment_monthly_statement_account.last_lending_activity_time_stamp = repayment_token_reserve.last_lending_activity_time_stamp;
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
        liquidati_repayment_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount; //Update liquidati monthly statement repayment amount, but not the tab. The tab is for the leader board and the liquidati shouldn't get credit for repayment in this case, but updating the monthly statement atleast for visibility to the liquidati.
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
        liquidator_liquidation_monthly_statement_account.monthly_fees_generated_amount += liquidation_fee_amount;

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
        liquidator_repayment_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        liquidator_repayment_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        liquidator_repayment_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
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
        liquidati_monthly_statement_account.monthly_repaid_debt_amount += repayment_amount; //Update liquidati monthly statement repayment amount, but not the tab. The tab is for the leader board and the liquidati shouldn't get credit for repayment in this case, but updating the monthly statement atleast for visibility to the liquidati.
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
        liquidator_monthly_statement_account.monthly_fees_generated_amount += liquidation_fee_amount;

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
        //liquidati_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp; //Since the token is the same, make Liquidate the last activity on the Monthly Statement
        liquidati_monthly_statement_account.last_lending_activity_amount = liquidation_amount;
        liquidati_monthly_statement_account.last_lending_activity_type = Activity::Liquidate as u8;
        liquidati_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp;
        //liquidator_monthly_statement_account.last_lending_activity_amount = repayment_amount;
        //liquidator_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        //liquidator_monthly_statement_account.last_lending_activity_time_stamp = token_reserve.last_lending_activity_time_stamp; //Since the token is the same, make Liquidate the last activity on the Monthly Statement
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
    //Feed in all of the Token Reserves remaining accounts as the same order as the token_reserve_mint_addresses input, then
    //Repeating sets of these remaining accounts in this order (Successfully tested with 10 tab account sets at once): LendingUserTabAccount, Submarket, LendingUserMonthlyStatementAccount
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

        let mut token_reserves: Vec<(&AccountInfo, Structs::TokenReserve)> = Vec::with_capacity(refresh_token_reserve_count.into());
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

            let unvalidated_lending_user_tab_account = Structs::LendingUserTabAccount::try_deserialize(&mut data_slice)?;

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
            let mut new_account_name_to_use: String = String::from("Solvency Treasury");
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
            let mut new_account_name_to_use: String = String::from("HODL Treasury");
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