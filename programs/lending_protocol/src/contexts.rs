use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface, TokenAccount};
use core::mem::size_of;
use crate::structs as Structs;

//Lending User Account need atleast 4 extra bytes of space to pass with full load(Longest name possible)
const LENDING_USER_ACCOUNT_EXTRA_SIZE: usize = 4;

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
        space = size_of::<Structs::LendingProtocol>() + 8)]
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump,
        space = size_of::<Structs::LendingProtocolCEO>() + 8)]
    pub ceo: Account<'info, Structs::LendingProtocolCEO>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"solvencyTreasurer".as_ref()],
        bump,
        space = size_of::<Structs::SolvencyTreasurer>() + 8)]
    pub solvency_treasurer: Account<'info, Structs::SolvencyTreasurer>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"liquidationTreasurer".as_ref()],
        bump,
        space = size_of::<Structs::LiquidationTreasurer>() + 8)]
    pub liquidation_treasurer: Account<'info, Structs::LiquidationTreasurer>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"oraclePriceValidator".as_ref()],
        bump,
        space = size_of::<Structs::OraclePriceValidator>() + 8)]
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingStats".as_ref()],
        bump,
        space = size_of::<Structs::LendingStats>() + 8)]
    pub lending_stats: Account<'info, Structs::LendingStats>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"lendingUserStats".as_ref()],
        bump,
        space = size_of::<Structs::LendingUserStats>() + 8)]
    pub lending_user_stats: Account<'info, Structs::LendingUserStats>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"tokenReserveStats".as_ref()],
        bump,
        space = size_of::<Structs::TokenReserveStats>() + 8)]
    pub token_reserve_stats: Account<'info, Structs::TokenReserveStats>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"subMarketStats".as_ref()],
        bump,
        space = size_of::<Structs::SubMarketStats>() + 8)]
    pub sub_market_stats: Account<'info, Structs::SubMarketStats>,

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
    pub ceo: Account<'info, Structs::LendingProtocolCEO>,

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
    pub solvency_treasurer: Account<'info, Structs::SolvencyTreasurer>,

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
    pub liquidation_treasurer: Account<'info, Structs::LiquidationTreasurer>,

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
    pub ceo: Account<'info, Structs::LendingProtocolCEO>,

    #[account(
        mut,
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(payload: Structs::PriceDataPayload)]
pub struct CreateTempOraclePriceData<'info> 
{
    ///CHECK: This is the address of the lending user requesting the price data
    pub lending_user_address: UncheckedAccount<'info>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"oraclePriceData".as_ref(), lending_user_address.key().as_ref()], 
        bump,
        space = (payload.data.len() * 17) + 1 + 4 + 8 + 8)]//Token Prices Count * (token_id(1byte) + normalized_price_18_decimals(16bytes) = 17bytes)
        //1(Bump) + 4(Borsh Vector Prefix) + 8(slot) + 8(Anchor Discriminator)
    pub temp_price_account: Account<'info, Structs::TempOraclePriceAccount>,

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
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"oraclePriceData".as_ref(), signer.key().as_ref()], 
        bump)]
    pub temp_price_account: Account<'info, Structs::TempOraclePriceAccount>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, Structs::LendingProtocolCEO>,

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
    pub token_reserve_stats: Account<'info, Structs::TokenReserveStats>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, Structs::LendingProtocolCEO>,

    #[account(
        init, 
        payer = signer,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump, 
        space = size_of::<Structs::TokenReserve>() + 8)]
    pub token_reserve: Account<'info, Structs::TokenReserve>,

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
    pub token_reserve_stats: Account<'info, Structs::TokenReserveStats>,

    #[account(
        seeds = [b"lendingProtocolCEO".as_ref()],
        bump)]
    pub ceo: Account<'info, Structs::LendingProtocolCEO>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Account<'info, Structs::TokenReserve>,

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
    pub sub_market_stats: Account<'info, Structs::SubMarketStats>,

    #[account(
        init,
        payer = signer,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), signer.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump, 
        space = size_of::<Structs::SubMarket>() + 8)]
    pub sub_market: Account<'info, Structs::SubMarket>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"subMarketOwnerLookUpTable".as_ref(), signer.key().as_ref()], 
        bump, 
        space = size_of::<Structs::SubMarketOwnerLookUpTable>() + 8)]
    pub sub_market_owner_look_up_table: Account<'info, Structs::SubMarketOwnerLookUpTable>,

    //The Token Reserve must exist to create a SubMarket. Only the ceo can create a Token Reserve.
    #[account(
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Account<'info, Structs::TokenReserve>,

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
    pub sub_market_stats: Account<'info, Structs::SubMarketStats>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_id.to_le_bytes().as_ref(), signer.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, Structs::SubMarket>,

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
    pub lending_protocol: Box<Account<'info, Structs::LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, Structs::LendingStats>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, Structs::SubMarket>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_user_stats: Account<'info, Structs::LendingUserStats>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
    pub lending_protocol: Box<Account<'info, Structs::LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, Structs::LendingStats>>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Box<Account<'info, Structs::OraclePriceValidator>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, Structs::SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Box<Account<'info, Structs::LendingUserAccount>>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Box<Account<'info, Structs::LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, Structs::LendingStats>>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Box<Account<'info, Structs::OraclePriceValidator>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, Structs::SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Box<Account<'info, Structs::LendingUserAccount>>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, Structs::LendingStats>>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Box<Account<'info, Structs::OraclePriceValidator>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>, 

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Box<Account<'info, Structs::SubMarket>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_account: Box<Account<'info, Structs::LendingUserAccount>>,

    #[account(
        mut,
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_reserve.token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        signer.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
    pub lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Box<Account<'info, Structs::LendingProtocol>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), repayment_mint.key().as_ref()], 
        bump)]
    pub repayment_token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), liquidation_mint.key().as_ref()], 
        bump)]
    pub liquidation_token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), liquidator_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub liquidator_lending_account: Box<Account<'info, Structs::LendingUserAccount>>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub liquidator_repayment_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub liquidator_liquidation_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_repayment_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_liquidation_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), liquidati_account_owner.key().as_ref(), liquidati_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub liquidati_lending_account: Box<Account<'info, Structs::LendingUserAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), liquidator_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub liquidator_lending_account: Box<Account<'info, Structs::LendingUserAccount>>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub liquidator_repayment_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub liquidator_liquidation_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_repayment_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_liquidation_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), liquidati_account_owner.key().as_ref(), liquidati_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub liquidati_lending_account: Box<Account<'info, Structs::LendingUserAccount>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), liquidator_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub liquidator_lending_account: Box<Account<'info, Structs::LendingUserAccount>>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub liquidator_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub liquidator_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        seeds = [b"oraclePriceValidator".as_ref()],
        bump)]
    pub price_validator: Account<'info, Structs::OraclePriceValidator>,

    #[account(
        mut,
        seeds = [b"lendingUserAccount".as_ref(), lending_user_owner.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        seeds = [b"lendingUserTabAccount".as_ref(),
        token_id.to_le_bytes().as_ref(),
        sub_market_owner.key().as_ref(),
        sub_market_index.to_le_bytes().as_ref(),
        lending_user_owner.key().as_ref(),
        user_account_index.to_le_bytes().as_ref()], 
        bump)]
    pub lending_user_tab_account: Account<'info, Structs::LendingUserTabAccount>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, Structs::LendingUserMonthlyStatementAccount>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, Structs::LendingStats>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, Structs::SubMarket>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Account<'info, Structs::LendingUserTabAccount>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, Structs::LendingUserMonthlyStatementAccount>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Account<'info, Structs::LendingStats>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()],
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), initial_sub_market_owner.key().as_ref(), initial_sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub initial_sub_market: Box<Account<'info, Structs::SubMarket>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), destination_sub_market_owner.key().as_ref(), destination_sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub destination_sub_market: Box<Account<'info, Structs::SubMarket>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub initial_lending_user_tab_account: Account<'info, Structs::LendingUserTabAccount>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub destination_lending_user_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub initial_lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub destination_lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Box<Account<'info, Structs::LendingProtocol>>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, Structs::LendingStats>>,

    #[account(
        seeds = [b"solvencyTreasurer".as_ref()],
        bump)]
    pub solvency_treasurer: Account<'info, Structs::SolvencyTreasurer>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Box<Account<'info, Structs::LendingUserTabAccount>>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Box<Account<'info, Structs::LendingUserMonthlyStatementAccount>>,

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
    pub lending_protocol: Account<'info, Structs::LendingProtocol>,

    #[account(
        mut, 
        seeds = [b"lendingStats".as_ref()],
        bump)]
    pub lending_stats: Box<Account<'info, Structs::LendingStats>>,

    #[account(
        seeds = [b"liquidationTreasurer".as_ref()],
        bump)]
    pub liquidation_treasurer: Account<'info, Structs::LiquidationTreasurer>,

    #[account(
        mut,
        seeds = [b"tokenReserve".as_ref(), token_mint_address.key().as_ref()], 
        bump)]
    pub token_reserve: Box<Account<'info, Structs::TokenReserve>>,

    #[account(
        mut,
        seeds = [b"subMarket".as_ref(), token_reserve.token_id.to_le_bytes().as_ref(), sub_market_owner.key().as_ref(), sub_market_index.to_le_bytes().as_ref()], 
        bump)]
    pub sub_market: Account<'info, Structs::SubMarket>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [b"lendingUserAccount".as_ref(), signer.key().as_ref(), user_account_index.to_le_bytes().as_ref()],
        bump, 
        space = size_of::<Structs::LendingUserAccount>() + LENDING_USER_ACCOUNT_EXTRA_SIZE + 8)]
    pub lending_user_account: Account<'info, Structs::LendingUserAccount>,

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
        space = size_of::<Structs::LendingUserTabAccount>() + 8)]
    pub lending_user_tab_account: Account<'info, Structs::LendingUserTabAccount>,

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
        space = size_of::<Structs::LendingUserMonthlyStatementAccount>() + 8)]
    pub lending_user_monthly_statement_account: Account<'info, Structs::LendingUserMonthlyStatementAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>
}