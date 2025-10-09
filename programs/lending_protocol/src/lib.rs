use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, SyncNative, CloseAccount};
use anchor_lang::system_program::{self};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;

declare_id!("7Jh9CEcMpaNsazT3DgvTmhp9MCnryVdFA2DQ6RRbW7hD");

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

//const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("Fdqu1muWocA5ms8VmTrUxRxxmSattrmpNraQ7RpPvzZg");
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
    NotCEO
}

#[error_code]
pub enum InvalidInputError
{
    #[msg("The fee on interest earned rate can't be greater than 100%")]
    InvalidFeeRate,
    #[msg("You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose")]
    InsufficientFunds,
    #[msg("You must provide all of the sub user's obligation accounts")]
    IncorrectNumberOfObligationAccounts,
    #[msg("You must provide the sub user's obligation accounts ordered by user_obligation_account_index")]
    IncorrectOrderOfObligationAccounts,
    #[msg("Unexpected Obligation Account PDA detected. Feed in only legitimate PDA's ordered by user_obligation_account_index")]
    UnexpectedObligationAccount,
    #[msg("Lending User Account name can't be longer than 25 characters")]
    LendingUserAccountNameTooLong,
}

#[program]
pub mod lending_protocol 
{
    use super::*;

    pub fn initialize_lending_protocol(ctx: Context<InitializeLendingProtocol>, tax_year: u32) -> Result<()> 
    {
        //Only the initial CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), INITIAL_CEO_ADDRESS, AuthorizationError::NotCEO);

        let ceo = &mut ctx.accounts.ceo;
        ceo.address = INITIAL_CEO_ADDRESS;

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.current_tax_year = tax_year;

        msg!("Lending Protocol Initialized");
        msg!("New CEO Address: {}", ceo.address.key());
        msg!("Current Tax Year: {}", lending_protocol.current_tax_year);

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

    pub fn update_current_tax_year(ctx: Context<UpdateCurrentTaxYear>, tax_year: u32) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.current_tax_year = tax_year;

        msg!("Updated Lending Protocol tax year to: {}", lending_protocol.current_tax_year);

        Ok(())
    }

    pub fn add_token_reserve(ctx: Context<AddTokenReserve>, token_mint_address: Pubkey, token_decimal_amount: u8) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let token_reserve_stats = &mut ctx.accounts.token_reserve_stats;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.token_mint_address = token_mint_address;
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.token_reserve_protocol_index = token_reserve_stats.token_reserve_count;

        token_reserve_stats.token_reserve_count += 1;

        msg!("Added Token Reserve #{}", token_reserve_stats.token_reserve_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
            
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
        let lending_user_stats = &mut ctx.accounts.lending_user_stats;
        let user_lending_account = &mut ctx.accounts.user_lending_account;
        let lending_user_obligation_account = &mut ctx.accounts.lending_user_obligation_account;
        let lending_user_yearly_tax_account = &mut ctx.accounts.lending_user_yearly_tax_account;

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
        
        //Populate obligation account if being newly initliazed. Every token the lending user enteracts with has its own obligation account tied to that sub user based on account index.
        if lending_user_obligation_account.user_obligation_account_added == false
        {
            lending_user_obligation_account.owner = ctx.accounts.signer.key();
            lending_user_obligation_account.user_account_index = user_account_index;
            lending_user_obligation_account.token_mint_address = token_mint_address;
            lending_user_obligation_account.sub_market_owner_address = sub_market_owner_address.key();
            lending_user_obligation_account.sub_market_index = sub_market_index;
            lending_user_obligation_account.user_obligation_account_index = user_lending_account.obligation_account_count;
            lending_user_obligation_account.user_obligation_account_added = true;

            user_lending_account.obligation_account_count += 1;

            msg!("Created Lending User Obligation Account Indexed At: {}", lending_user_obligation_account.user_obligation_account_index);
        }

        //Initialize yearly tax account if first time deposit or the tax year has changed.
        if lending_user_yearly_tax_account.yearly_tax_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_yearly_tax_account.owner = ctx.accounts.signer.key();
            lending_user_yearly_tax_account.user_account_index = user_account_index;
            lending_user_yearly_tax_account.token_mint_address = token_mint_address;
            lending_user_yearly_tax_account.tax_year = lending_protocol.current_tax_year;
            lending_user_yearly_tax_account.yearly_tax_account_added = true;

            msg!("Created Tax Account for year: {}", lending_user_yearly_tax_account.tax_year);
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

            lending_user_stats.deposits += 1;
            sub_market.deposited_amount += amount as u128;
            token_reserve.deposited_amount += amount as u128;
            lending_user_obligation_account.deposited_amount += amount as u128;

            msg!("{} deposited for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);
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

            lending_user_stats.deposits += 1;
            sub_market.deposited_amount += amount as u128;
            token_reserve.deposited_amount += amount as u128;
            lending_user_obligation_account.deposited_amount += amount as u128;
            
            msg!("{} deposited for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);   
        }

        lending_user_yearly_tax_account.last_activity_type = Activity::Deposit as u8;
        lending_user_yearly_tax_account.last_lending_activity_time_stamp = Clock::get()?.unix_timestamp as u64;

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
        let lending_user_obligation_account = &mut ctx.accounts.lending_user_obligation_account;
        //You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose
        require!(lending_user_obligation_account.deposited_amount >= amount as u128, InvalidInputError::InsufficientFunds);

         let user_lending_account = &mut ctx.accounts.user_lending_account;
        //You must provide all of the sub user's obligation accounts in remaining accounts
        require!(user_lending_account.obligation_account_count as usize == ctx.remaining_accounts.len(), InvalidInputError::IncorrectNumberOfObligationAccounts);

        let sub_market = &mut ctx.accounts.sub_market;
        let lending_user_stats = &mut ctx.accounts.lending_user_stats;
        let lending_user_yearly_tax_account = &mut ctx.accounts.lending_user_yearly_tax_account;

        let mut user_obligation_index = 0;

        //Validate Passed In User Obligation Accounts
        for remaining_account in ctx.remaining_accounts.iter()
        {
            let data_ref = remaining_account.data.borrow();
            let mut data_slice: &[u8] = data_ref.deref();

            let obligation_account = LendingUserObligationAccount::try_deserialize(&mut data_slice)?;

            let (expected_pda, _bump) = Pubkey::find_program_address(
                &[b"lendingUserObligationAccount",
                obligation_account.token_mint_address.key().as_ref(),
                obligation_account.sub_market_owner_address.key().as_ref(),
                obligation_account.sub_market_index.to_le_bytes().as_ref(),
                ctx.accounts.signer.key().as_ref(),
                user_account_index.to_le_bytes().as_ref()],
                &ctx.program_id,
            );

            //You must provide all of the sub user's obligation accounts ordered by user_obligation_account_index
            require!(user_obligation_index == obligation_account.user_obligation_account_index, InvalidInputError::IncorrectOrderOfObligationAccounts);
            require_keys_eq!(expected_pda.key(), remaining_account.key(), InvalidInputError::UnexpectedObligationAccount);

            user_obligation_index += 1;
        }

        //Initialize yearly tax account if the tax year has changed.
        if lending_user_yearly_tax_account.yearly_tax_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_yearly_tax_account.owner = ctx.accounts.signer.key();
            lending_user_yearly_tax_account.user_account_index = user_account_index;
            lending_user_yearly_tax_account.token_mint_address = token_mint_address;
            lending_user_yearly_tax_account.tax_year = lending_protocol.current_tax_year;
            lending_user_yearly_tax_account.yearly_tax_account_added = true;

            msg!("Created Tax Account for year: {}", lending_user_yearly_tax_account.tax_year);
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

        lending_user_stats.withdrawals += 1;
        sub_market.deposited_amount -= amount as u128;
        token_reserve.deposited_amount -= amount as u128;
        lending_user_obligation_account.deposited_amount -= amount as u128;

        lending_user_yearly_tax_account.last_activity_type = Activity::Withdraw as u8;
        lending_user_yearly_tax_account.last_lending_activity_time_stamp = Clock::get()?.unix_timestamp as u64;
        
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
        let lending_user_yearly_tax_account = &mut ctx.accounts.lending_user_yearly_tax_account;

        //Initialize yearly tax account if the tax year has changed.
        if lending_user_yearly_tax_account.yearly_tax_account_added == false
        {
            let lending_protocol = & ctx.accounts.lending_protocol;

            lending_user_yearly_tax_account.owner = ctx.accounts.signer.key();
            lending_user_yearly_tax_account.user_account_index = user_account_index;
            lending_user_yearly_tax_account.token_mint_address = token_mint_address;
            lending_user_yearly_tax_account.tax_year = lending_protocol.current_tax_year;
            lending_user_yearly_tax_account.yearly_tax_account_added = true;

            msg!("Created Tax Account for year: {}", lending_user_yearly_tax_account.tax_year);
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

        lending_user_yearly_tax_account.last_activity_type = Activity::Repay as u8;
        lending_user_yearly_tax_account.last_lending_activity_time_stamp = Clock::get()?.unix_timestamp as u64;
  
        msg!("{} repaied for token mint address: {}", ctx.accounts.signer.key(), token_reserve.token_mint_address);

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
pub struct UpdateCurrentTaxYear<'info> 
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
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(
        mut, 
        seeds = [b"lendingUserStats".as_ref()],
        bump)]
    pub lending_user_stats: Account<'info, LendingUserStats>,

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
    pub user_lending_account: Account<'info, LendingUserAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserObligationAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserObligationAccount>() + 8)]
    pub lending_user_obligation_account: Account<'info, LendingUserObligationAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserYearlyTaxAccount".as_ref(),
        lending_protocol.current_tax_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserYearlyTaxAccount>() + 8)]
    pub lending_user_yearly_tax_account: Account<'info, LendingUserYearlyTaxAccount>,

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
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(
        mut, 
        seeds = [b"lendingUserStats".as_ref()],
        bump)]
    pub lending_user_stats: Account<'info, LendingUserStats>,

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
        seeds = [b"lendingUserObligationAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_obligation_account: Account<'info, LendingUserObligationAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserYearlyTaxAccount".as_ref(),
        lending_protocol.current_tax_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserYearlyTaxAccount>() + 8)]
    pub lending_user_yearly_tax_account: Account<'info, LendingUserYearlyTaxAccount>,

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
        seeds = [b"lendingUserStats".as_ref()],
        bump)]
    pub lending_user_stats: Account<'info, LendingUserStats>,

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
        seeds = [b"lendingUserObligationAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_obligation_account: Account<'info, LendingUserObligationAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserYearlyTaxAccount".as_ref(),
        lending_protocol.current_tax_year.to_le_bytes().as_ref(),
        token_mint_address.key().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<LendingUserYearlyTaxAccount>() + 8)]
    pub lending_user_yearly_tax_account: Account<'info, LendingUserYearlyTaxAccount>,

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
    pub current_tax_year: u32
}

#[account]
pub struct TokenReserveStats
{
    pub token_reserve_count: u32
}

#[account]
pub struct SubMarketStats //Moved these lending protocol variables here to help stream line the listeners on the front end, so that when ever there is any change what so ever on this account, we can be sure that we need to do a .all() for the SubMarket accounts on the front end without having to fetch some other account to check a different number before hand. Less fetches/alls, the better.
{
    pub sub_market_creation_count: u32,
    pub sub_market_edit_count: u32
}

#[account]
pub struct LendingUserStats
{
    pub deposits: u128,
    pub withdrawals: u128,
    pub repayments: u128,
    pub liquidations: u128,
    pub swaps: u128
}

#[account]
pub struct TokenReserve
{
    pub token_reserve_protocol_index: u32,
    pub token_mint_address: Pubkey,
    pub token_decimal_amount: u8,
    pub deposited_amount: u128
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
    pub deposited_amount: u128
}

#[account]
pub struct LendingUserAccount //Giving the lending account an index to allow users to have multiple lending accounts if they so choose, so they don't have to use multiple wallets
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub account_name: String,
    pub lending_user_account_added: bool,
    pub obligation_account_count: u32,
    pub interest_accrued: u128,
    pub debt_repaid: u128,
    pub amount_liquidated: u128
}

#[account]
pub struct LendingUserObligationAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub token_mint_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u16,
    pub user_obligation_account_index: u32,
    pub user_obligation_account_added: bool,
    pub deposited_amount: u128,
    pub borrowed_amount: u128
}

#[account]
pub struct LendingUserYearlyTaxAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub token_mint_address: Pubkey,
    pub tax_year: u32,
    pub yearly_tax_account_added: bool,
    pub last_activity_type: u8,
    pub last_lending_activity_time_stamp: u64,
    pub interest_accrued: u128,
    pub debt_repaid: u128,
    pub amount_liquidated: u128
}