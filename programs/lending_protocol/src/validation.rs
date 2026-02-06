use anchor_lang::prelude::*;
use crate::*;
use crate::errors::InvalidInputError;

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
  .map_err(|_| InvalidInputError::UnexpectedLendingStatsAccount)?;

  //Verify Lending User Tab Account Address is the expected PDA
  require_keys_eq!(expected_pda.key(), lending_stats_serialized.key(), InvalidInputError::UnexpectedLendingStatsAccount);

  Ok(lending_stats)
}

pub fn validate_and_return_token_reserve_account<'info>(
    program_id: Pubkey,
    token_reserve_account_serialized: &AccountInfo<'info>,
    token_mint_address: Pubkey) -> Result<TokenReserve>
{
    let mut data_slice: &[u8] = &token_reserve_account_serialized.data.borrow();

    let token_reserve = TokenReserve::try_deserialize(&mut data_slice)?;

    let seeds = &
    [
        b"tokenReserve",
        token_mint_address.as_ref(),
        &[token_reserve.bump]
    ];

    //Verify SubMarket PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| InvalidInputError::UnexpectedTokenReserveAccount)?;

    //Verify SubMarket Address is the expected PDA
    require_keys_eq!(expected_pda.key(), token_reserve_account_serialized.key(), InvalidInputError::UnexpectedTokenReserveAccount);

    Ok(token_reserve)
}

pub fn validate_and_return_sub_market_account<'info>(
    program_id: Pubkey,
    sub_market_account_serialized: &AccountInfo<'info>,
    token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16) -> Result<SubMarket>
{
    let mut data_slice: &[u8] = &sub_market_account_serialized.data.borrow();

    let sub_market = SubMarket::try_deserialize(&mut data_slice)?;
    let sub_market_index_to_le_bytes = sub_market_index.to_le_bytes();

    let seeds = &
    [
        b"subMarket",
        token_mint_address.as_ref(),
        sub_market_owner_address.as_ref(),
        sub_market_index_to_le_bytes.as_ref(),
        &[sub_market.bump]
    ];

    //Verify SubMarket PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| InvalidInputError::UnexpectedSubMarketAccount)?;

    //Verify SubMarket Address is the expected PDA
    require_keys_eq!(expected_pda.key(), sub_market_account_serialized.key(), InvalidInputError::UnexpectedSubMarketAccount);

    Ok(sub_market)
}

pub fn validate_and_return_lending_user_tab_account<'info>(
    program_id: Pubkey,
    tab_account_serialized: &AccountInfo<'info>,
    token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner_address: Pubkey,
    user_account_index: u8) -> Result<LendingUserTabAccount>
{
    let mut data_slice: &[u8] = &tab_account_serialized.data.borrow();

    let lending_user_tab_account = LendingUserTabAccount::try_deserialize(&mut data_slice)?;

    let user_account_index_to_le_bytes = user_account_index.to_le_bytes();
    let sub_market_index_to_le_bytes = sub_market_index.to_le_bytes();
    let seeds = &
    [
        b"lendingUserTabAccount",
        token_mint_address.as_ref(),
        sub_market_owner_address.as_ref(),
        sub_market_index_to_le_bytes.as_ref(),
        user_account_owner_address.as_ref(),
        user_account_index_to_le_bytes.as_ref(),
        &[lending_user_tab_account.bump]
    ];

    //Verify Lending User Tab Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| InvalidInputError::UnexpectedTabAccount)?;

    //Verify Lending User Tab Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), tab_account_serialized.key(), InvalidInputError::UnexpectedTabAccount);

    Ok(lending_user_tab_account)
}

pub fn validate_and_return_lending_user_monthly_state_account<'info>(
    program_id: Pubkey,
    monthly_statement_account_serialized: &AccountInfo<'info>,
    current_statement_month: u8,
    current_statement_year: u16,
    token_mint_address: Pubkey,
    sub_market_owner_address: Pubkey,
    sub_market_index: u16,
    user_account_owner_address: Pubkey,
    user_account_index: u8) -> Result<LendingUserMonthlyStatementAccount>
{
    let mut data_slice: &[u8] = &monthly_statement_account_serialized.data.borrow();

    let monthly_statement_account = LendingUserMonthlyStatementAccount::try_deserialize(&mut data_slice)?;

    let current_statement_month_to_le_bytes = current_statement_month.to_le_bytes();
    let current_statement_year_to_le_bytes = current_statement_year.to_le_bytes();
    let sub_market_index_to_le_bytes = sub_market_index.to_le_bytes();
    let user_account_index_to_le_bytes = user_account_index.to_le_bytes();
    let seeds = &
    [
        b"userMonthlyStatementAccount",
        current_statement_month_to_le_bytes.as_ref(),
        current_statement_year_to_le_bytes.as_ref(),
        token_mint_address.as_ref(),
        sub_market_owner_address.as_ref(),
        sub_market_index_to_le_bytes.as_ref(),
        user_account_owner_address.as_ref(),
        user_account_index_to_le_bytes.as_ref(),
        &[monthly_statement_account.bump]
    ];

    //Verify Monthly Statement Account PDA is a valid PDA
    let expected_pda = Pubkey::create_program_address(seeds, &program_id)
    .map_err(|_| InvalidInputError::UnexpectedMonthlyStatementAccount)?;

    //Verify Monthly Statement Account Address is the expected PDA
    require_keys_eq!(expected_pda.key(), monthly_statement_account_serialized.key(), InvalidInputError::UnexpectedMonthlyStatementAccount);

    Ok(monthly_statement_account)
}