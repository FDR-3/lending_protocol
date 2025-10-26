use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, SyncNative, CloseAccount};
use anchor_lang::system_program::{self};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;
use spl_math::precise_number::PreciseNumber;

declare_id!("3EdkWLvZttPawtx2V1GjisrM2BCwKw6BdnZW9XiFArHr");

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

const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("Fdqu1muWocA5ms8VmTrUxRxxmSattrmpNraQ7RpPvzZg");
//const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("DSLn1ofuSWLbakQWhPUenSBHegwkBBTUwx8ZY4Wfoxm");

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
    NotCEO
}

#[error_code]
pub enum InvalidInputError
{
    #[msg("The fee on interest earned rate can't be greater than 100%")]
    InvalidFeeRate,
    #[msg("You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose")]
    InsufficientFunds,
    #[msg("You must provide all of the sub user's tab accounts")]
    IncorrectNumberOfTabAccounts,
    #[msg("You must provide the sub user's tab accounts ordered by user_tab_account_index")]
    IncorrectOrderOfTabAccounts,
    #[msg("Unexpected Tab Account PDA detected. Feed in only legitimate PDA's ordered by user_tab_account_index")]
    UnexpectedTabAccount,
    #[msg("Lending User Account name can't be longer than 25 characters")]
    LendingUserAccountNameTooLong,
}

//Helper function to update token reserve Accrued Interest Index before a lending transaction (deposit, withdraw, borrow, repay, liqudate)
fn update_token_accrued_interest_index_and_user_tab<'info>(token_reserve: &mut Account<TokenReserve>, new_lending_activity_time_stamp: u64) -> Result<()>
{
    //Use spl-math library PreciseNumber for fixed point math
    //Set Token Reserve Accured Interest Index = Old Accured Interest Index * (1 + Supply APY * Δt/Seconds in a Year)
    let old_accured_interest_index_precise  = PreciseNumber::new(token_reserve.accrued_interest_index).unwrap();
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

    token_reserve.accrued_interest_index = new_accured_interest_index_precise.to_imprecise().unwrap();

    Ok(())
}

//Helper function to update token reserve rates after a lending transaction (deposit, withdraw, borrow, repay, liqudate)
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

    pub fn add_token_reserve(ctx: Context<AddTokenReserve>, token_mint_address: Pubkey, token_decimal_amount: u8, borrow_apy: u16, global_limit: u128) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.token_mint_address = token_mint_address;
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.borrow_apy = borrow_apy;
        token_reserve.global_limit = global_limit;

        token_reserve.token_reserve_protocol_index = token_reserve_stats.token_reserve_count;
        token_reserve_stats.token_reserve_count += 1;

        msg!("Added Token Reserve #{}", token_reserve_stats.token_reserve_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
        msg!("Borrow APY: {}",  borrow_apy);
        msg!("Global Limit: {}",  global_limit);
            
        Ok(())
    }

    pub fn update_token_reserve(ctx: Context<UpdateTokenReserve>, _token_mint_address: Pubkey, borrow_apy: u16, global_limit: u128) -> Result<()> 
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
            lending_user_tab_account.sub_market_owner_address = sub_market_owner_address.key();
            lending_user_tab_account.sub_market_index = sub_market_index;
            lending_user_tab_account.user_tab_account_index = user_lending_account.tab_account_count;
            lending_user_tab_account.user_tab_account_added = true;

            user_lending_account.tab_account_count += 1;

            msg!("Created Lending User Tab Account Indexed At: {}", lending_user_tab_account.user_tab_account_index);
        }

        //Initialize monthly statement account if first time deposit or the statement year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_monthly_statement_account.owner = ctx.accounts.signer.key();
            lending_user_monthly_statement_account.user_account_index = user_account_index;
            lending_user_monthly_statement_account.token_mint_address = token_mint_address;
            lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
            lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
            lending_user_monthly_statement_account.monthly_statement_account_added = true;

            msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);
        }

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

        //Update deposited amounts
        lending_stats.deposits += 1;
        sub_market.deposited_amount += amount as u128;
        token_reserve.deposited_amount += amount as u128;
        lending_user_tab_account.deposited_amount += amount as u128;
        lending_user_monthly_statement_account.monthly_deposited_amount += amount as u128;
        lending_user_monthly_statement_account.life_time_balance_amount = lending_user_tab_account.deposited_amount;
        msg!("{} deposited for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);   

        let time_stamp = Clock::get()?.unix_timestamp as u64;

        update_token_accrued_interest_index_and_user_tab(token_reserve, time_stamp)?;
        update_token_reserve_rates(token_reserve)?;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount as u128;
        token_reserve.last_lending_activity_type = Activity::Deposit as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = amount as u128;
        sub_market.last_lending_activity_type = Activity::Deposit as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp; 
        user_lending_account.last_lending_activity_token_mint_address = token_mint_address.key();
        user_lending_account.last_lending_activity_amount = amount as u128;
        user_lending_account.last_lending_activity_type = Activity::Deposit as u8;
        user_lending_account.last_lending_activity_time_stamp = time_stamp;
        lending_user_tab_account.last_lending_activity_amount = amount as u128;
        lending_user_tab_account.last_lending_activity_type = Activity::Deposit as u8;
        lending_user_tab_account.last_lending_activity_time_stamp = time_stamp;
        lending_user_monthly_statement_account.last_lending_activity_amount = amount as u128;
        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Deposit as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = time_stamp;

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

        update_token_accrued_interest_index_and_user_tab(token_reserve, time_stamp)?;

        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        //You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose
        require!(lending_user_tab_account.deposited_amount >= amount as u128, InvalidInputError::InsufficientFunds);

         let user_lending_account = &mut ctx.accounts.user_lending_account;
        //You must provide all of the sub user's tab accounts in remaining accounts
        require!(user_lending_account.tab_account_count as usize == ctx.remaining_accounts.len(), InvalidInputError::IncorrectNumberOfTabAccounts);

        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;

        let mut user_tab_index = 0;

        //Validate Passed In User Tab Accounts
        for remaining_account in ctx.remaining_accounts.iter()
        {
            let data_ref = remaining_account.data.borrow();
            let mut data_slice: &[u8] = data_ref.deref();

            let tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

            let (expected_pda, _bump) = Pubkey::find_program_address(
                &[b"lendingUserTabAccount",
                tab_account.token_mint_address.key().as_ref(),
                tab_account.sub_market_owner_address.key().as_ref(),
                tab_account.sub_market_index.to_le_bytes().as_ref(),
                ctx.accounts.signer.key().as_ref(),
                user_account_index.to_le_bytes().as_ref()],
                &ctx.program_id,
            );

            //You must provide all of the sub user's tab accounts ordered by user_tab_account_index
            require!(user_tab_index == tab_account.user_tab_account_index, InvalidInputError::IncorrectOrderOfTabAccounts);
            require_keys_eq!(expected_pda.key(), remaining_account.key(), InvalidInputError::UnexpectedTabAccount);

            user_tab_index += 1;
        }

        //Initialize monthly statement account if the statement year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_monthly_statement_account.owner = ctx.accounts.signer.key();
            lending_user_monthly_statement_account.user_account_index = user_account_index;
            lending_user_monthly_statement_account.token_mint_address = token_mint_address;
            lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
            lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
            lending_user_monthly_statement_account.monthly_statement_account_added = true;

            msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);
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
        
        //Update deposited amounts
        lending_stats.withdrawals += 1;
        sub_market.deposited_amount -= amount as u128;
        token_reserve.deposited_amount -= amount as u128;
        lending_user_tab_account.deposited_amount -= amount as u128;
        lending_user_monthly_statement_account.monthly_withdrawal_amount += amount as u128;
        lending_user_monthly_statement_account.life_time_balance_amount = lending_user_tab_account.deposited_amount;
        
        update_token_reserve_rates(token_reserve)?;

        //Update last activity on accounts
        token_reserve.last_lending_activity_amount = amount as u128;
        token_reserve.last_lending_activity_type = Activity::Withdraw as u8;
        token_reserve.last_lending_activity_time_stamp = time_stamp;
        sub_market.last_lending_activity_amount = amount as u128;
        sub_market.last_lending_activity_type = Activity::Withdraw as u8;
        sub_market.last_lending_activity_time_stamp = time_stamp; 
        user_lending_account.last_lending_activity_token_mint_address = token_mint_address.key();
        user_lending_account.last_lending_activity_amount = amount as u128;
        user_lending_account.last_lending_activity_type = Activity::Withdraw as u8;
        user_lending_account.last_lending_activity_time_stamp = time_stamp;
        lending_user_tab_account.last_lending_activity_amount = amount as u128;
        lending_user_tab_account.last_lending_activity_type = Activity::Withdraw as u8;
        lending_user_tab_account.last_lending_activity_time_stamp = time_stamp;
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
        let lending_user_tab_account = &mut ctx.accounts.lending_user_tab_account;
        //You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose
        require!(lending_user_tab_account.deposited_amount >= amount as u128, InvalidInputError::InsufficientFunds);

         let user_lending_account = &mut ctx.accounts.user_lending_account;
        //You must provide all of the sub user's tab accounts in remaining accounts
        require!(user_lending_account.tab_account_count as usize == ctx.remaining_accounts.len(), InvalidInputError::IncorrectNumberOfTabAccounts);

        let sub_market = &mut ctx.accounts.sub_market;
        let lending_stats = &mut ctx.accounts.lending_stats;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;

        let mut user_tab_index = 0;

        //Validate Passed In User Tab Accounts
        for remaining_account in ctx.remaining_accounts.iter()
        {
            let data_ref = remaining_account.data.borrow();
            let mut data_slice: &[u8] = data_ref.deref();

            let tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

            let (expected_pda, _bump) = Pubkey::find_program_address(
                &[b"lendingUserTabAccount",
                tab_account.token_mint_address.key().as_ref(),
                tab_account.sub_market_owner_address.key().as_ref(),
                tab_account.sub_market_index.to_le_bytes().as_ref(),
                ctx.accounts.signer.key().as_ref(),
                user_account_index.to_le_bytes().as_ref()],
                &ctx.program_id,
            );

            //You must provide all of the sub user's tab accounts ordered by user_tab_account_index
            require!(user_tab_index == tab_account.user_tab_account_index, InvalidInputError::IncorrectOrderOfTabAccounts);
            require_keys_eq!(expected_pda.key(), remaining_account.key(), InvalidInputError::UnexpectedTabAccount);

            user_tab_index += 1;
        }

        //Initialize monthly statement account if the statement year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_monthly_statement_account.owner = ctx.accounts.signer.key();
            lending_user_monthly_statement_account.user_account_index = user_account_index;
            lending_user_monthly_statement_account.token_mint_address = token_mint_address;
            lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
            lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
            lending_user_monthly_statement_account.monthly_statement_account_added = true;

            msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);
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

        lending_stats.withdrawals += 1;
        sub_market.deposited_amount -= amount as u128;
        token_reserve.deposited_amount -= amount as u128;
        lending_user_tab_account.deposited_amount -= amount as u128;

        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Withdraw as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = Clock::get()?.unix_timestamp as u64;
        
        msg!("{} withdrew for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

        Ok(())
    }

    pub fn repay_tokens(ctx: Context<RepayTokens>,
        token_mint_address: Pubkey,
        _sub_market_owner_address: Pubkey,
        _sub_market_index: u16,
        user_account_index: u8,
        amount: u64
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let lending_user_monthly_statement_account = &mut ctx.accounts.lending_user_monthly_statement_account;

        //Initialize monthly statement account if the statement year has changed.
        if lending_user_monthly_statement_account.monthly_statement_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_monthly_statement_account.owner = ctx.accounts.signer.key();
            lending_user_monthly_statement_account.user_account_index = user_account_index;
            lending_user_monthly_statement_account.token_mint_address = token_mint_address;
            lending_user_monthly_statement_account.statement_month = lending_protocol.current_statement_month;
            lending_user_monthly_statement_account.statement_year = lending_protocol.current_statement_year;
            lending_user_monthly_statement_account.monthly_statement_account_added = true;

            msg!("Created Statement Account for month: {}, year: {}", lending_user_monthly_statement_account.statement_month, lending_user_monthly_statement_account.statement_year);
        }

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

        lending_user_monthly_statement_account.last_lending_activity_type = Activity::Repay as u8;
        lending_user_monthly_statement_account.last_lending_activity_time_stamp = Clock::get()?.unix_timestamp as u64;
  
        msg!("{} repaid debt for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

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
    pub token_reserve: Account<'info, TokenReserve>,

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
    pub token_reserve: Account<'info, TokenReserve>,

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
    pub repayments: u128,
    pub liquidations: u128,
    pub swaps: u128
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
    pub supply_apy: u128,
    pub borrow_apy: u16,
    pub utilization_rate: u64,
    pub global_limit: u128,
    pub accrued_interest_index: u128, //Starts at 1 (in fixed point notation) and increases as supply interest is earned from Borrow Users so that it can be proportionally distributed to Supply Users
    pub interest_accrued: u128,
    pub debt_repaid: u128,
    pub amount_liquidated: u128,
    pub deposited_amount: u128,
    pub borrowed_amount: u128,
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
    pub interest_accrued: u128,
    pub debt_repaid: u128,
    pub amount_liquidated: u128,
    pub deposited_amount: u128,
    pub borrowed_amount: u128,
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
    pub interest_accrued: u128,
    pub debt_repaid: u128,
    pub amount_liquidated: u128,
    pub last_lending_activity_token_mint_address: Pubkey,
    pub last_lending_activity_amount: u128,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64
}

#[account]
pub struct LendingUserTabAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub token_mint_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub user_tab_account_index: u32,
    pub user_tab_account_added: bool,
    pub interest_accrued: u128,
    pub debt_repaid: u128,
    pub amount_liquidated: u128,
    pub deposited_amount: u128,
    pub borrowed_amount: u128,
    pub last_lending_activity_amount: u128,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64 
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
    pub life_time_balance_amount: u128,//The life_time properties give a snapshot of the value of the Tab Account at the time it is updated
    pub life_time_borrowed_amount: u128,
    pub life_time_interest_accrued_amount: u128,
    pub life_time_debt_repaid_amount: u128,
    pub life_time_liquidated_amount: u128,
    pub monthly_deposited_amount: u128,//The monthly properties give the specific value changes for that specific month
    pub monthly_withdrawal_amount: u128,
    pub monthly_borrowed_amount: u128,
    pub monthly_repaid_amount: u128,
    pub monthly_liquidated_amount: u128,
    pub last_lending_activity_amount: u128,
    pub last_lending_activity_type: u8,
    pub last_lending_activity_time_stamp: u64 
}