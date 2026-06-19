use anchor_lang::prelude::*;
use crate::*;
use crate::errors::LendingError;

pub fn validate_and_return_price_validator_account<'info>(program_id: Pubkey, price_validator_serialized: &AccountInfo<'info>) -> Result<OraclePriceValidator>
{
    let mut data_slice: &[u8] = &price_validator_serialized.data.borrow();

    let price_validator = OraclePriceValidator::try_deserialize(&mut data_slice)?;

    let bump = [price_validator.bump];
    let seeds = &
    [
        b"oraclePriceValidator".as_ref(),//It seems when there is only the string for the seed, you need the .as_ref() on it and the bump
        &bump.as_ref()
    ];

    //Verify Lending User Tab Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedOraclePriceValidatorAccount)?;
        
    //Verify Lending User Tab Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), price_validator_serialized.key(), LendingError::UnexpectedOraclePriceValidatorAccount);

    Ok(price_validator)
}

pub fn validate_and_return_lending_stats_account<'info>(program_id: Pubkey, lending_stats_serialized: &AccountInfo<'info>) -> Result<LendingStats>
{
    let mut data_slice: &[u8] = &lending_stats_serialized.data.borrow();

    let lending_stats = LendingStats::try_deserialize(&mut data_slice)?;

    let bump = [lending_stats.bump];
    let seeds = &
    [
        b"lendingStats".as_ref(),//It seems when there is only the string for the seed, you need the .as_ref() on it and the bump
        &bump.as_ref()
    ];

    //Verify Lending User Tab Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedLendingStatsAccount)?;
        
    //Verify Lending User Tab Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), lending_stats_serialized.key(), LendingError::UnexpectedLendingStatsAccount);

    Ok(lending_stats)
}

pub fn validate_and_return_token_reserve_account<'info>(
    program_id: Pubkey,
    token_reserve_account_serialized: &AccountInfo<'info>) -> Result<TokenReserve>
{
    let mut data_slice: &[u8] = &token_reserve_account_serialized.data.borrow();

    let token_reserve = TokenReserve::try_deserialize(&mut data_slice)?;

    let seeds = &
    [
        b"tokenReserve",
        token_reserve.token_mint_address.as_ref(), //Using the mint address from the account. Token Reserve accounts can only be created by the CEO and checks in refresh_user_health_chunk_and_token_reserves that ensure the token_mint_address is correct one by cross references it with the lending user tab account
        &[token_reserve.bump]
    ];

    //Verify Token Reserve PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedTokenReserveAccount)?;

    //Verify Token Reserve Address is the expected PDA
    require_keys_eq!(expected_pda.key(), token_reserve_account_serialized.key(), LendingError::UnexpectedTokenReserveAccount);

    Ok(token_reserve)
}

pub fn validate_token_reserve_ata<'info>(
    ata_account_info: &AccountInfo<'info>,
    expected_mint: Pubkey,
    expected_authority: Pubkey) -> Result<()>
{
    let mut data = &ata_account_info.data.borrow()[..];
    let token_account = TokenAccount::try_deserialize(&mut data)
        .map_err(|_| LendingError::InvalidTokenAccount)?;

    //3. Verify Mint
    require_keys_eq!(token_account.mint, expected_mint, LendingError::InvalidTokenAccountMint);

    //4. Verify Authority (Owner)
    require_keys_eq!(token_account.owner, expected_authority, LendingError::InvalidTokenAccountOwner);

    Ok(())
}

pub fn validate_and_return_sub_market_account<'info>(
    program_id: Pubkey,
    sub_market_account_serialized: &AccountInfo<'info>,
    token_id: u8,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16) -> Result<SubMarket>
{
    let mut data_slice: &[u8] = &sub_market_account_serialized.data.borrow();

    let token_id_to_le_bytes = token_id.to_le_bytes();
    let sub_market = SubMarket::try_deserialize(&mut data_slice)?;
    let sub_market_index_to_le_bytes = sub_market_index.to_le_bytes();

    let seeds = &
    [
        b"subMarket",
        token_id_to_le_bytes.as_ref(),
        sub_market_owner_address.as_ref(),
        sub_market_index_to_le_bytes.as_ref(),
        &[sub_market.bump]
    ];

    //Verify SubMarket PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedSubMarketAccount)?;

    //Verify SubMarket Address is the expected PDA
    require_keys_eq!(expected_pda.key(), sub_market_account_serialized.key(), LendingError::UnexpectedSubMarketAccount);

    Ok(sub_market)
}

pub fn validate_and_return_lending_user_account<'info>(
    program_id: Pubkey,
    lending_user_account_serialized: &AccountInfo<'info>,
    user_account_owner_address: Pubkey,
    user_account_index: u8) -> Result<LendingUserAccount>
{
    let mut data_slice: &[u8] = &lending_user_account_serialized.data.borrow();

    let lending_user_account = LendingUserAccount::try_deserialize(&mut data_slice)?;

    let user_account_index_to_le_bytes = user_account_index.to_le_bytes();

    let seeds = &
    [
        b"lendingUserAccount",
        user_account_owner_address.as_ref(),
        user_account_index_to_le_bytes.as_ref(),
        &[lending_user_account.bump]
    ];

    //Verify Lending User Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedLendingUserAccount)?;

    //Verify Lending User Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), lending_user_account_serialized.key(), LendingError::UnexpectedLendingUserAccount);

    Ok(lending_user_account)
}

pub fn validate_and_return_lending_user_tab_account<'info>(
    program_id: Pubkey,
    tab_account_serialized: &AccountInfo<'info>,
    token_id: u8,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner_address: Pubkey,
    user_account_index: u8) -> Result<LendingUserTabAccount>
{
    let mut data_slice: &[u8] = &tab_account_serialized.data.borrow();

    let lending_user_tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

    let token_id_to_le_bytes = token_id.to_le_bytes();
    let user_account_index_to_le_bytes = user_account_index.to_le_bytes();
    let sub_market_index_to_le_bytes = sub_market_index.to_le_bytes();
    let seeds = &
    [
        b"lendingUserTabAccount",
        token_id_to_le_bytes.as_ref(),
        sub_market_owner_address.as_ref(),
        sub_market_index_to_le_bytes.as_ref(),
        user_account_owner_address.as_ref(),
        user_account_index_to_le_bytes.as_ref(),
        &[lending_user_tab_account.bump]
    ];

    //Verify Lending User Tab Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedTabAccount)?;

    //Verify Lending User Tab Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), tab_account_serialized.key(), LendingError::UnexpectedTabAccount);

    Ok(lending_user_tab_account)
}

pub fn validate_and_return_lending_user_monthly_state_account<'info>(
    program_id: Pubkey,
    monthly_statement_account_serialized: &AccountInfo<'info>,
    current_statement_month: u8,
    current_statement_year: u16,
    token_id: u8,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner_address: Pubkey,
    user_account_index: u8) -> Result<LendingUserMonthlyStatementAccount>
{
    let mut data_slice: &[u8] = &monthly_statement_account_serialized.data.borrow();

    let monthly_statement_account = LendingUserMonthlyStatementAccount::try_deserialize(&mut data_slice)?;

    let current_statement_month_to_le_bytes = current_statement_month.to_le_bytes();
    let current_statement_year_to_le_bytes = current_statement_year.to_le_bytes();
    let token_id_to_le_bytes = token_id.to_le_bytes();
    let sub_market_index_to_le_bytes = sub_market_index.to_le_bytes();
    let user_account_index_to_le_bytes = user_account_index.to_le_bytes();
    let seeds = &
    [
        b"userMonthlyStatementAccount",
        current_statement_month_to_le_bytes.as_ref(),
        current_statement_year_to_le_bytes.as_ref(),
        token_id_to_le_bytes.as_ref(),
        sub_market_owner_address.as_ref(),
        sub_market_index_to_le_bytes.as_ref(),
        user_account_owner_address.as_ref(),
        user_account_index_to_le_bytes.as_ref(),
        &[monthly_statement_account.bump]
    ];

    //Verify Monthly Statement Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| LendingError::UnexpectedMonthlyStatementAccount)?;

    //Verify Monthly Statement Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), monthly_statement_account_serialized.key(), LendingError::UnexpectedMonthlyStatementAccount);

    Ok(monthly_statement_account)
}