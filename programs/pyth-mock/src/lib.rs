use anchor_lang::prelude::*;
use std::io::Write;

declare_id!("9inp4dycaG3Ktg2c3qijFHGfc3Y64Jzmsqsj79E4P5wh");

#[program]
pub mod pyth_mock 
{
    use super::*;

    pub fn set_mocked_pyth_price_update_account(ctx: Context<SetMockPythPriceUpdateData>, data: Vec<u8>) -> Result<()>
    {
        let account_data = ctx.accounts.mocked_pyth_price_update_pda.to_account_info().data;
        let borrow_data = &mut *account_data.borrow_mut();

        Ok((&mut borrow_data[0..]).write_all(&data[..])?)
    }
}

#[derive(Accounts)]
pub struct SetMockPythPriceUpdateData<'info> 
{
    #[account(mut)]
    mocked_pyth_price_update_pda: Signer<'info>,
}