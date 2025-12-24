use anchor_lang::prelude::*;
use std::io::Write;

declare_id!("EoRKL64KbTVsg5FQke7AAHcuMiNWVTcyC4JdkLeChSWc");

#[program]
pub mod pyth_mock 
{
    use super::*;

    pub fn set_mocked_pyth_price_update_account(ctx: Context<SetMockPythPriceUpdateData>, data: Vec<u8>) -> Result<()>
    {
        let account_data = ctx.accounts.mocked_pyth_price_update_account.to_account_info().data;
        let borrow_data = &mut *account_data.borrow_mut();

        Ok((&mut borrow_data[0..]).write_all(&data[..])?)
    }
}

#[derive(Accounts)]
pub struct SetMockPythPriceUpdateData<'info> 
{
    //Using a generic signer account instead of an anchor struct so that anchor doesn't force account discriminator to be something specific
    #[account(mut)]
    mocked_pyth_price_update_account: Signer<'info>,
}