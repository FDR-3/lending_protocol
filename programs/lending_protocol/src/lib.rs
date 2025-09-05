use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};
use core::mem::size_of;
use solana_security_txt::security_txt;
use std::ops::Deref;

declare_id!("9yGaDci3e79TskjUZjPBS5HHP9JSetyJbgizgTqWfBRn");

#[cfg(not(feature = "no-entrypoint"))] // Ensure it's not included when compiled as a library
security_txt! {
    name: "Lending Protocol",
    project_url: "https://m4a.io",
    contacts: "email fdr3@m4a.io",
    preferred_languages: "en",
    source_code: "https://github.com/FDR-3?tab=repositories",
    policy: "If you find a bug, email me and say something please D:"
}

const INITIAL_CEO_ADDRESS: Pubkey = pubkey!("Fdqu1muWocA5ms8VmTrUxRxxmSattrmpNraQ7RpPvzZg");

//Error Codes
#[error_code]
pub enum AuthorizationError 
{
    #[msg("Only the CEO can call this function")]
    NotCEO,
    #[msg("Only the Submarket Owner can call this function")]
    NotSubMarketOwner
}

#[error_code]
pub enum InvalidInputError
{
    #[msg("The fee on interest earned rate can't be greater than 100% or less than 0%")]
    InvalidFeeRate,
    #[msg("You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose")]
    InsufficientFunds,
    #[msg("You must provide all of the sub user's obligation accounts.")]
    IncorrectNumberOfObligationAccounts,
    #[msg("You must provide the sub user's obligation accounts ordered by user_obligation_account_index")]
    IncorrectOrderOfObligationAccounts,
    #[msg("Unexpected Obligation Account PDA detected. Feed in only legitimate PDA's ordered by user_obligation_account_index")]
    UnexpectedObligationAccount
}

#[program]
pub mod lending_protocol 
{
    use super::*;

    pub fn initialize_lending_protocol(ctx: Context<InitializeLendingProtocol>) -> Result<()> 
    {
        //Only the initial CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), INITIAL_CEO_ADDRESS, AuthorizationError::NotCEO);

        let ceo = &mut ctx.accounts.ceo;
        ceo.address = INITIAL_CEO_ADDRESS;

        msg!("Lending Protocol Initialized");
        msg!("New CEO Address: {}", ceo.address.key());

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

    pub fn add_token_reserve(ctx: Context<AddTokenReserve>, token_mint_address: Pubkey, token_decimal_amount: u8) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        let token_reserve = &mut ctx.accounts.token_reserve;
        token_reserve.token_mint_address = token_mint_address;
        token_reserve.token_decimal_amount = token_decimal_amount;
        token_reserve.token_reserve_protocol_index = lending_protocol.token_reserve_count;

        lending_protocol.token_reserve_count += 1;

        msg!("Added Token Reserve #{}", lending_protocol.token_reserve_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("Token Decimal Amount: {}", token_decimal_amount);
            
        Ok(())
    }

    /*pub fn delete_token_reserve(ctx: Context<DeleteTokenReserve>, token_mint_address: Pubkey) -> Result<()> 
    {
        let ceo = &mut ctx.accounts.ceo;
        //Only the CEO can call this function
        require_keys_eq!(ctx.accounts.signer.key(), ceo.address.key(), AuthorizationError::NotCEO);

        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.token_reserve_count -= 1;

        msg!("Deleted Token Reserve");
        msg!("Token Mint Address: {}", token_mint_address.key());
            
        Ok(())
    }*/

    pub fn create_sub_market(ctx: Context<CreateSubMarket>,
        token_mint_address: Pubkey,
        sub_market_index: u8,
        fee_collector_address: Pubkey,
        fee_on_interest_earned_rate: f32
    ) -> Result<()> 
    {
        //Fee on interest earned rate can't be greater than 100%, 1 in decimal form
        require!(fee_on_interest_earned_rate <= 1.0000, InvalidInputError::InvalidFeeRate);

        //Fee on interest earned rate can't be less than 0%, 0 in decimal form
        require!(fee_on_interest_earned_rate >= 0.0000, InvalidInputError::InvalidFeeRate);

        let sub_market = &mut ctx.accounts.sub_market;
        sub_market.owner = ctx.accounts.signer.key();
        sub_market.fee_collector_address = fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate; //This should fed in as a decimal from 0.0000 to 1.0000
        sub_market.token_mint_address = token_mint_address.key(); //This can't be edited after. Allowing this to be edited would be like allowing some one to say this currency is a different kind of currency later when ever they wanted
        
        let lending_protocol = &mut ctx.accounts.lending_protocol;
        lending_protocol.sub_market_count += 1;

        msg!("Created SubMarket #{}", lending_protocol.sub_market_count);
        msg!("Token Mint Address: {}", token_mint_address.key());
        msg!("SubMarket Index: {}", sub_market_index);
        msg!("Owner: {}", ctx.accounts.signer.key());
        msg!("Fee Collector Address: {}", fee_collector_address.key());
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate*100.0); //convert out of % fixed point notation with 4 decimal places back to decimal for logging
        
        Ok(())
    }

    pub fn edit_sub_market(ctx: Context<EditSubMarket>,
        _token_mint_address: Pubkey,
        sub_market_index: u8,
        fee_collector_address: Pubkey,
        fee_on_interest_earned_rate: f32
    ) -> Result<()> 
    {
        //Fee on interest earned rate can't be greater than 100%, 1 in decimal form
        require!(fee_on_interest_earned_rate <= 1.0000, InvalidInputError::InvalidFeeRate);

        //Fee on interest earned rate can't be less than 0%, 0 in decimal form
        require!(fee_on_interest_earned_rate >= 0.0000, InvalidInputError::InvalidFeeRate);

        let sub_market = &mut ctx.accounts.sub_market;
        //Only the sub market owner can call this function
        require_keys_eq!(ctx.accounts.signer.key(), sub_market.owner.key(), AuthorizationError::NotSubMarketOwner);

        sub_market.fee_collector_address = fee_collector_address.key();
        sub_market.fee_on_interest_earned_rate = fee_on_interest_earned_rate;

        msg!("Edited Submarket");
        msg!("Token Mint Address: {}", sub_market.token_mint_address.key());
        msg!("SubMarket Index: {}", sub_market_index);
        msg!("Owner: {}", ctx.accounts.signer.key());
        msg!("Fee Collector Address: {}", fee_collector_address.key());
        msg!("Fee On Interest Earned Rate: {:.2}%", fee_on_interest_earned_rate*100.0); //convert out of fixed point notation with 4 decimal places back to percent for logging. So / 10^4 for decimal then * 10^2 for percent
            
        Ok(())
    }

    pub fn deposit_tokens(ctx: Context<DepositTokens>,
        token_mint_address: Pubkey,
        sub_market_owner_address: Pubkey,
        sub_market_index: u8,
        account_index: u8,
        amount: f64
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let user_account = &mut ctx.accounts.user_account;
        let user_token_obligation_account = &mut ctx.accounts.user_token_obligation_account;

        //Populate obligation account if being newly initliazed. A user can have multiple accounts based on their account index. Every token the sub user enteracts with has its own obligation account tied to the sub user.
        if user_token_obligation_account.user_obligation_account_added == false
        {
            user_token_obligation_account.owner = ctx.accounts.signer.key();
            user_token_obligation_account.user_account_index = account_index;
            user_token_obligation_account.token_mint_address = token_mint_address;
            user_token_obligation_account.sub_market_owner_address = sub_market_owner_address.key();
            user_token_obligation_account.sub_market_index = sub_market_index;
            user_token_obligation_account.user_obligation_account_index = user_account.obligation_account_count;

            user_token_obligation_account.user_obligation_account_added = true;

            user_account.obligation_account_count += 1;
        }

        //Cross Program Invocation for Token Transfer
        let cpi_accounts = token::Transfer
        {
            from: ctx.accounts.user_ata.to_account_info(),
            to: ctx.accounts.token_reserve_ata.to_account_info(),
            authority: ctx.accounts.signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        let base_int :u64 = 10;
        let conversion_number = base_int.pow(token_reserve.token_decimal_amount as u32) as f64;
        let fixed_pointed_notation_amount = (amount * conversion_number) as u64;

        //Transfer Tokens Into The Reserve
        token::transfer(cpi_ctx, fixed_pointed_notation_amount)?;

        user_token_obligation_account.deposited_amount += fixed_pointed_notation_amount;
        token_reserve.deposited_amount += fixed_pointed_notation_amount;
        
        msg!("Successfully deposited ${:.token_decimal_amount$} tokens for mint address: {}", amount, token_reserve.token_mint_address, token_decimal_amount = token_reserve.token_decimal_amount as usize);

        Ok(())
    }

    pub fn withdraw_tokens(ctx: Context<WithdrawTokens>,
        _token_mint_address: Pubkey,
        _sub_market_owner_address: Pubkey,
        _sub_market_index: u8,
        account_index: u8,
        amount: f64
    ) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;
        let user_account = &mut ctx.accounts.user_account;
        let user_token_obligation_account = &mut ctx.accounts.user_token_obligation_account;

        //You must provide all of the sub user's obligation accounts in remaining accounts
        require!(user_account.obligation_account_count as usize == ctx.remaining_accounts.len(), InvalidInputError::IncorrectNumberOfObligationAccounts);

        let mut user_obligation_index = 0;

        //Validate User Obligation Accounts
        for remaining_account in ctx.remaining_accounts.iter()
        {
            let data_ref = remaining_account.data.borrow();
            let mut data_slice: &[u8] = data_ref.deref();

            let obligation_account = UserTokenObligationAccount::try_deserialize(&mut data_slice)?;

            let (expected_pda, _bump) = Pubkey::find_program_address(
                &[b"userTokenObligationAccount",
                obligation_account.token_mint_address.key().as_ref(),
                obligation_account.sub_market_owner_address.key().as_ref(),
                obligation_account.sub_market_index.to_le_bytes().as_ref(),
                ctx.accounts.signer.key().as_ref(),
                account_index.to_le_bytes().as_ref()],
                &ctx.program_id,
            );

            //You must provide all of the sub user's obligation accounts ordered by user_obligation_account_index
            require!(user_obligation_index == obligation_account.user_obligation_account_index, InvalidInputError::IncorrectOrderOfObligationAccounts);
            require_keys_eq!(expected_pda.key(), remaining_account.key(), InvalidInputError::UnexpectedObligationAccount);

            user_obligation_index += 1;
        }

        //Cross Program Invocation for Token Transfer
        let cpi_accounts = token::Transfer
        {
            from: ctx.accounts.token_reserve_ata.to_account_info(),
            to: ctx.accounts.user_ata.to_account_info(),
            authority: ctx.accounts.signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        let base_int :u64 = 10;
        let conversion_number = base_int.pow(token_reserve.token_decimal_amount as u32) as f64;
        let fixed_pointed_notation_amount = (amount * conversion_number) as u64;

        //You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose
        require!(user_token_obligation_account.deposited_amount > fixed_pointed_notation_amount, InvalidInputError::InsufficientFunds);
        
        //Transfer Tokens Into The Reserve
        token::transfer(cpi_ctx, fixed_pointed_notation_amount)?;

        user_token_obligation_account.deposited_amount -= fixed_pointed_notation_amount;
        token_reserve.deposited_amount -= fixed_pointed_notation_amount;
        
        msg!("Successfully withdrew ${:.token_decimal_amount$} tokens for mint address: {}", amount, token_reserve.token_mint_address, token_decimal_amount = token_reserve.token_decimal_amount as usize);

        Ok(())
    }

    pub fn repay_tokens(ctx: Context<RepayTokens>, _token_mint_address: Pubkey, _sub_market_owner_address: Pubkey, _sub_market_index: u8, amount: f64) -> Result<()> 
    {
        let token_reserve = &mut ctx.accounts.token_reserve;

        //Cross Program Invocation for Token Transfer
        let cpi_accounts = token::Transfer
        {
            from: ctx.accounts.user_ata.to_account_info(),
            to: ctx.accounts.token_reserve_ata.to_account_info(),
            authority: ctx.accounts.signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        let base_int :u64 = 10;
        let conversion_number = base_int.pow(token_reserve.token_decimal_amount as u32) as f64;
        let fixed_pointed_notation_amount = (amount * conversion_number) as u64;

        //Transfer Tokens Into The Reserve
        token::transfer(cpi_ctx, fixed_pointed_notation_amount)?;
        
        msg!("Successfully repayed ${:.token_decimal_amount$} tokens for mint address: {}", amount, token_reserve.token_mint_address, token_decimal_amount = token_reserve.token_decimal_amount as usize);

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
#[instruction(token_mint_address: Pubkey)]
pub struct AddTokenReserve<'info> 
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

    #[account(
        init, 
        payer = signer,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump, 
        space = size_of::<TokenReserve>() + 8)]
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey)]
pub struct RemoveTokenReserve<'info> 
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

    #[account(
        mut,
        close = signer,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Account<'info, TokenReserve>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(token_mint_address: Pubkey, sub_market_index: u8)]
pub struct CreateSubMarket<'info> 
{
    #[account(
        mut,
        seeds = [b"lendingProtocol".as_ref()],
        bump)]
    pub lending_protocol: Account<'info, LendingProtocol>,

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
#[instruction(token_mint_address: Pubkey, sub_market_index: u8)]
pub struct EditSubMarket<'info> 
{
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
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u8, account_index: u8)]
pub struct DepositTokens<'info> 
{
    
    #[account(
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
        seeds = [b"userAccount".as_ref(), signer.key().as_ref(), account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<UserAccount>() + 8)]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"userTokenObligationAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        account_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<UserTokenObligationAccount>() + 8)]
    pub user_token_obligation_account: Account<'info, UserTokenObligationAccount>,

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
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u8, account_index: u8)]
pub struct WithdrawTokens<'info> 
{
    
    #[account(
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
        seeds = [b"userAccount".as_ref(), signer.key().as_ref(), account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        mut,
        seeds = [b"userTokenObligationAccount".as_ref(),
        token_mint_address.key().as_ref(),
        sub_market_owner_address.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        account_index.to_le_bytes().as_ref()], 
        bump)]
    pub user_token_obligation_account: Account<'info, UserTokenObligationAccount>,

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
#[instruction(token_mint_address: Pubkey, sub_market_owner_address: Pubkey, sub_market_index: u8)]
pub struct RepayTokens<'info> 
{
    
    #[account(
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
    pub token_reserve_count: u32,
    pub sub_market_count: u32 //You can techincally get this number with the .all() function on the front end and just looking at the length of the array, but keeping this number in the logging atleast tells you the chronologically order that account was created in. Although you could also store that on the submarket itself, but it would make all of those accounts bigger, and you'd still need this variable to keep track of the count.
}

#[account]
pub struct TokenReserve
{
    pub token_reserve_protocol_index: u32,
    pub token_mint_address: Pubkey,
    pub token_decimal_amount: u8,
    pub deposited_amount: u64
}

#[account]
pub struct SubMarket
{
    pub owner: Pubkey,
    pub token_mint_address: Pubkey,
    pub fee_collector_address: Pubkey,
    pub fee_on_interest_earned_rate: f32
}

#[account]
pub struct UserAccount //Giving the user account an index to allow users to have multiple accounts if they so choose
{
    pub owner: Pubkey,
    pub account_index: u8,
    pub obligation_account_count: u32,
    pub deposited_value_usd: u64,
    pub borrowed_value_usd: u64
}

#[account]
pub struct UserTokenObligationAccount
{
    pub owner: Pubkey,
    pub user_account_index: u8,
    pub token_mint_address: Pubkey,
    pub sub_market_owner_address: Pubkey,
    pub sub_market_index: u8,
    pub user_obligation_account_index: u32,
    pub user_obligation_account_added: bool,
    pub deposited_amount: u64,
    pub borrowed_amount: u64
}