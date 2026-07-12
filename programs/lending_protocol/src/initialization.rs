use anchor_lang::prelude::*;
use crate::errors::LendingError;
use crate::structs as Structs;
use crate::shared_constants::MAX_ACCOUNT_NAME_LENGTH;

//Helper function to initialize Lending User Account
pub fn initialize_lending_user_account<'info>(lending_user_account: &mut Structs::LendingUserAccount,
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
    msg!("Set Lending User Look Up Table Address: {}", lending_user_account.look_up_table_address);

    Ok(())
}

//Helper function to initialize Lending User Tab Account
pub fn initialize_lending_user_tab_account<'info>(lending_user_account: &mut Structs::LendingUserAccount,
    lending_user_tab_account: &mut Structs::LendingUserTabAccount,
    lending_protocol: &Structs::LendingProtocol,
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
pub fn initialize_lending_user_monthly_statement_account<'info>(lending_user_monthly_statement_account: &mut Structs::LendingUserMonthlyStatementAccount,
    lending_user_tab_account: &Structs::LendingUserTabAccount,
    lending_protocol: &Structs::LendingProtocol,
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