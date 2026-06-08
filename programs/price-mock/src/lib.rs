use anchor_lang::prelude::*;
use switchboard_on_demand::{SwitchboardQuote, PackedQuoteHeader};
use switchboard_on_demand::on_demand::oracle_quote::feed_info::PackedFeedInfo;
use switchboard_on_demand::sysvar::ed25519_sysvar::Ed25519SignatureOffsets;
use switchboard_on_demand::on_demand::oracle_quote::quote_account::OracleSignature;
use switchboard_on_demand::smallvec::{SmallVec, U16Prefix};
use anchor_lang::solana_program::sysvar::instructions::ID as INSTRUCTIONS_ID;
use anchor_lang::solana_program::sysvar::slot_hashes::ID as SLOT_HASHES_ID;
use anchor_lang::solana_program::hash::hash;
use std::io::Write;
use std::io::Cursor;

declare_id!("4HSLn75xsDdQuKuxKt2dy656EEU9iNZExtgYcrMVBUsN");

fn serialize_and_save_switchboard_quote_account<W: Write>(
    quote: &SwitchboardQuote, 
    writer: &mut W
) -> Result<()> {
    //1. queue: Pubkey (32 bytes)
    writer.write_all(quote.queue.as_ref())?;
        
    //2. signatures: SmallVec (u16 length prefix + items)
    //Manually write the u16 length
    writer.write_all(&(quote.signatures.len() as u16).to_le_bytes())?;

    //Needed for mocking signatures locally
    //Can be used to save populated signature data
    /*for sig in quote.signatures.iter()
    {
        //Manually write each OracleSignature field
        //writer.write_all(&[sig.offsets.signature_offset as u8])?; // Assuming u8/u16 based on struct
        //writer.write_all(&(sig.offsets.signature_instruction_index as u16).to_le_bytes())?;

        //Here for definitions
        let ed25519_signature_offsets = Ed25519SignatureOffsets
        {
            signature_offset: 0,
            signature_instruction_index: 0,
            public_key_offset: 0,
            public_key_instruction_index: 0,
            message_data_offset: 0,
            message_data_size: 0,
            message_instruction_index: 0
        };

        let oracle_signature = OracleSignature
        {
            offsets: ed25519_signature_offsets, //Offsets to locate signature data within instruction
            pubkey: *ctx.program_id, //ED25519 public key
            signature: [0u8; 64], //ED25519 signature (64 bytes)
        };
    }*/

    //3. quote_header: PackedQuoteHeader (32 bytes)
    writer.write_all(&quote.quote_header.signed_slothash)?;

    // 4. feeds: SmallVec (u8 length prefix + items)
    writer.write_all(&(quote.feeds.len() as u8).to_le_bytes())?;
    for feed in quote.feeds.iter() {
        // 1. feed_id (32 bytes)
        writer.write_all(&feed.feed_id)?;
        
        // 2. feed_value (i128 - 16 bytes)
        writer.write_all(&feed.feed_value.to_le_bytes())?;
        
        // 3. min_oracle_samples (u8 - 1 byte)
        writer.write_all(&[feed.min_oracle_samples])?;
    }

    //5. oracle_idxs: SmallVec (u8 length prefix + bytes)
    writer.write_all(&(quote.oracle_idxs.len() as u8).to_le_bytes())?;
    writer.write_all(&quote.oracle_idxs)?;

    //6. slot: u64
    writer.write_all(&quote.slot.to_le_bytes())?;

    //7. version: u8
    writer.write_all(&[quote.version])?;

    //8. tail_discriminator: [u8; 4]
    writer.write_all(&quote.tail_discriminator)?;

    Ok(())
}

#[program]
pub mod price_mock 
{
    use super::*;

    pub fn set_mocked_pyth_price_update_data(ctx: Context<SetMockPythPriceUpdateData>, data: Vec<u8>) -> Result<()>
    {
        let account_data = ctx.accounts.mocked_pyth_price_update_account.to_account_info().data;
        let borrow_data = &mut *account_data.borrow_mut();

        Ok((&mut borrow_data[0..]).write_all(&data[..])?)
    }

    pub fn set_mocked_switchboard_quote_data(
        ctx: Context<SetMockSwitchboardQuoteData>,
        feed_ids: Vec<[u8; 32]>,
        feed_values: Vec<i128>,
        min_feed_oracle_samples: u8
    ) -> Result<()> {
        if feed_ids.len() != feed_values.len()
        {
            return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
        }

        let slot_bytes = ctx.accounts.clock.slot.to_le_bytes();
        let slot_hash_bytes = hash(&slot_bytes).to_bytes(); // Returns a [u8; 32]

        let packed_quote_header = PackedQuoteHeader
        {
            signed_slothash: slot_hash_bytes
        };
        
        let mut quote = SwitchboardQuote
        {
            queue: ctx.accounts.queue.key(),
            signatures: SmallVec::<OracleSignature, U16Prefix>::new(), 
            quote_header: packed_quote_header,
            feeds: SmallVec::<PackedFeedInfo>::new(),
            oracle_idxs: SmallVec::<u8>::new(),
            slot: ctx.accounts.clock.slot,
            version: 1,
            tail_discriminator: *b"SBOD",
        };

        //Needed for mocking signatures locally
        /*let ed25519_signature_offsets = Ed25519SignatureOffsets
        {
            signature_offset: 0,
            signature_instruction_index: 0,
            public_key_offset: 0,
            public_key_instruction_index: 0,
            message_data_offset: 0,
            message_data_size: 0,
            message_instruction_index: 0
        };

        let oracle_signature = OracleSignature
        {
            offsets: ed25519_signature_offsets, //Offsets to locate signature data within instruction
            pubkey: *ctx.program_id, //ED25519 public key
            signature: [0u8; 64], //ED25519 signature (64 bytes)
        };

        quote.signatures.push(oracle_signature);*/
        
        //Now push the feeds
        for(i, id) in feed_ids.iter().enumerate()
        {
            let feed = PackedFeedInfo
            {
                feed_id: *id,
                feed_value: feed_values[i],
                min_oracle_samples: min_feed_oracle_samples,
            };
            quote.feeds.push(feed);
        }
        
        //Serialize and save data
        let mut data = ctx.accounts.mocked_switchboard_quote_account.data.borrow_mut();
        let mut cursor = Cursor::new(&mut data[..]);

        // Call your helper
        serialize_and_save_switchboard_quote_account(&quote, &mut cursor)
            .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotSerialize)?;
        
        Ok(())
    }
}

#[derive(Accounts)]
pub struct SetMockPythPriceUpdateData<'info> 
{
    //Using a generic signer account instead of an anchor struct so that anchor doesn't force account discriminator to be something specific
    #[account(mut)]
    mocked_pyth_price_update_account: Signer<'info>
}

#[derive(Accounts)]
pub struct SetMockSwitchboardQuoteData<'info> 
{
    //Using a generic signer account instead of an anchor struct so that anchor doesn't force account discriminator to be something specific
    #[account(mut)]
    mocked_switchboard_quote_account: Signer<'info>,

    ///CHECK: Queue account used by Switchboard; Only be tested in a local environment
    pub queue: AccountInfo<'info>,
    //pub clock: Sysvar<'info, Clock>,
    ///CHECK: SlotHashes account used by Switchboard; Only be tested in a local environment
    #[account(address = SLOT_HASHES_ID)]
    pub slothashes: AccountInfo<'info>,
    ///CHECK: Instructions account used by Switchboard; Only be tested in a local environment
    #[account(address = INSTRUCTIONS_ID)]
    pub instructions: AccountInfo<'info>,

    pub clock: Sysvar<'info, Clock>
}