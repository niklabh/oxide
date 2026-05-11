use anchor_lang::prelude::*;

// Placeholder ID. Run `anchor keys sync` after the first `anchor build` to
// replace this with the real program ID generated under target/deploy/.
declare_id!("8CZfcw3uB6wXjmzsQaVmDwxEGvKirwbBpemZE8eF8Sjb");

pub const MAX_NAME_LEN: usize = 64;
pub const SEED_PREFIX: &[u8] = b"wasm";

#[program]
pub mod wasm_registry {
    use super::*;

    pub fn register(ctx: Context<Register>, name: String, hash: [u8; 32]) -> Result<()> {
        require!(!name.is_empty(), RegistryError::NameEmpty);
        require!(name.len() <= MAX_NAME_LEN, RegistryError::NameTooLong);

        let entry = &mut ctx.accounts.entry;
        let clock = Clock::get()?;

        entry.publisher = ctx.accounts.publisher.key();
        entry.hash = hash;
        entry.name = name;
        entry.version = 0;
        entry.created_at = clock.unix_timestamp;
        entry.updated_at = clock.unix_timestamp;
        entry.bump = ctx.bumps.entry;

        emit!(EntryRegistered {
            publisher: entry.publisher,
            name: entry.name.clone(),
            hash,
            version: 0,
        });
        Ok(())
    }

    pub fn update(ctx: Context<Update>, hash: [u8; 32]) -> Result<()> {
        let entry = &mut ctx.accounts.entry;
        entry.hash = hash;
        entry.version = entry
            .version
            .checked_add(1)
            .ok_or(RegistryError::VersionOverflow)?;
        entry.updated_at = Clock::get()?.unix_timestamp;

        emit!(EntryUpdated {
            publisher: entry.publisher,
            name: entry.name.clone(),
            hash,
            version: entry.version,
        });
        Ok(())
    }

    pub fn revoke(ctx: Context<Revoke>) -> Result<()> {
        emit!(EntryRevoked {
            publisher: ctx.accounts.entry.publisher,
            name: ctx.accounts.entry.name.clone(),
        });
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct Register<'info> {
    #[account(mut)]
    pub publisher: Signer<'info>,

    #[account(
        init,
        payer = publisher,
        space = WasmEntry::SPACE,
        seeds = [SEED_PREFIX, publisher.key().as_ref(), name.as_bytes()],
        bump,
    )]
    pub entry: Account<'info, WasmEntry>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Update<'info> {
    pub publisher: Signer<'info>,

    #[account(
        mut,
        seeds = [SEED_PREFIX, publisher.key().as_ref(), entry.name.as_bytes()],
        bump = entry.bump,
        has_one = publisher,
    )]
    pub entry: Account<'info, WasmEntry>,
}

#[derive(Accounts)]
pub struct Revoke<'info> {
    #[account(mut)]
    pub publisher: Signer<'info>,

    #[account(
        mut,
        seeds = [SEED_PREFIX, publisher.key().as_ref(), entry.name.as_bytes()],
        bump = entry.bump,
        has_one = publisher,
        close = publisher,
    )]
    pub entry: Account<'info, WasmEntry>,
}

#[account]
pub struct WasmEntry {
    pub publisher: Pubkey,
    pub hash: [u8; 32],
    pub name: String,
    pub version: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub bump: u8,
}

impl WasmEntry {
    // 8 discriminator
    // + 32 publisher
    // + 32 hash
    // + (4 + MAX_NAME_LEN) name (length prefix + bytes)
    // + 4 version
    // + 8 created_at
    // + 8 updated_at
    // + 1 bump
    pub const SPACE: usize = 8 + 32 + 32 + (4 + MAX_NAME_LEN) + 4 + 8 + 8 + 1;
}

#[event]
pub struct EntryRegistered {
    pub publisher: Pubkey,
    pub name: String,
    pub hash: [u8; 32],
    pub version: u32,
}

#[event]
pub struct EntryUpdated {
    pub publisher: Pubkey,
    pub name: String,
    pub hash: [u8; 32],
    pub version: u32,
}

#[event]
pub struct EntryRevoked {
    pub publisher: Pubkey,
    pub name: String,
}

#[error_code]
pub enum RegistryError {
    #[msg("Name must not be empty")]
    NameEmpty,
    #[msg("Name exceeds 64 bytes")]
    NameTooLong,
    #[msg("Version counter overflowed")]
    VersionOverflow,
}
