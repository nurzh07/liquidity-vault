use anchor_lang::prelude::*;

pub const OPEN_POSITION_V2: [u8; 8] = [77, 184, 74, 214, 112, 86, 241, 199];

pub const DECREASE_LIQUIDITY_V2: [u8; 8] = [58, 127, 188, 62, 79, 82, 196, 96];

pub const CLOSE_POSITION: [u8; 8] = [123, 134, 81, 0, 49, 68, 98, 98];

#[account]
#[derive(InitSpace)]
pub struct ClmmPosition {
    pub vault: Pubkey,
    pub pool: Pubkey,
    pub nft_mint: Pubkey,
    pub personal_position: Pubkey,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,
    pub active: bool,
    pub bump: u8,
}

pub fn open_position_data(
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower: i32,
    tick_upper: i32,
    tick_array_lower: i32,
    tick_array_upper: i32,
) -> Vec<u8> {
    let mut data = Vec::new();

    data.extend_from_slice(&OPEN_POSITION_V2);
    data.extend_from_slice(&liquidity.to_le_bytes());
    data.extend_from_slice(&amount_0_max.to_le_bytes());
    data.extend_from_slice(&amount_1_max.to_le_bytes());
    data.extend_from_slice(&tick_lower.to_le_bytes());
    data.extend_from_slice(&tick_upper.to_le_bytes());
    data.extend_from_slice(&tick_array_lower.to_le_bytes());
    data.extend_from_slice(&tick_array_upper.to_le_bytes());

    data.push(0); // with_metadata = false
    data.push(0); // base_flag = None

    data
}

pub fn decrease_liquidity_data(liquidity: u128, amount_0_min: u64, amount_1_min: u64) -> Vec<u8> {
    let mut data = Vec::new();

    data.extend_from_slice(&DECREASE_LIQUIDITY_V2);
    data.extend_from_slice(&liquidity.to_le_bytes());
    data.extend_from_slice(&amount_0_min.to_le_bytes());
    data.extend_from_slice(&amount_1_min.to_le_bytes());

    data
}

pub fn close_position_data() -> Vec<u8> {
    CLOSE_POSITION.to_vec()
}

use anchor_spl::token_interface::{self, Approve, Revoke};

pub fn approve_tokens<'info>(
    token_program: AccountInfo<'info>,
    token_account: AccountInfo<'info>,
    delegate: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let accounts = Approve {
        to: token_account,
        delegate,
        authority,
    };

    token_interface::approve(
        CpiContext::new(token_program.key(), accounts).with_signer(signer_seeds),
        amount,
    )
}

pub fn revoke_tokens<'info>(
    token_program: AccountInfo<'info>,
    token_account: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let accounts = Revoke {
        source: token_account,
        authority,
    };

    token_interface::revoke(
        CpiContext::new(token_program.key(), accounts).with_signer(signer_seeds),
    )
}

use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

pub fn invoke_clmm<'info>(
    program: AccountInfo<'info>,
    accounts: &[AccountInfo<'info>],
    metas: Vec<AccountMeta>,
    data: Vec<u8>,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    require_keys_eq!(program.key(), crate::CLMM_PROGRAM_ID);

    let ix = Instruction {
        program_id: crate::CLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let mut infos = accounts.to_vec();
    infos.push(program);

    invoke_signed(&ix, &infos, signer_seeds)?;

    Ok(())
}
