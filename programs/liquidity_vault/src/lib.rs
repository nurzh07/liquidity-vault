use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{
    self, Burn, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("C9pH3cWmgQihD2xPQgWxsb7QyWkNpXQNPxsCBkVJrQM1");

/// Программа Raydium CLMM в DEVNET (официальная таблица адресов Raydium):
/// DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH
pub const CLMM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    184, 152, 151, 52, 252, 179, 140, 145, 104, 216, 83, 199, 83, 182, 184, 164,
    54, 16, 205, 211, 37, 175, 187, 199, 47, 212, 21, 54, 219, 205, 194, 88,
]);

/// Идентификатор инструкции swap_v2 в CLMM (первые 8 байт sha256 от "global:swap_v2")
pub const SWAP_V2_DISCRIMINATOR: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];

#[program]
pub mod liquidity_vault {
    use super::*;

    /// Неделя 8: создаёт vault, share-токен и хранилище токенов.
    pub fn initialize_vault(ctx: Context<InitializeVault>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.admin = ctx.accounts.admin.key();
        vault.deposit_mint = ctx.accounts.deposit_mint.key();
        vault.share_mint = ctx.accounts.share_mint.key();
        vault.vault_token = ctx.accounts.vault_token.key();
        vault.total_deposited = 0;
        vault.bump = ctx.bumps.vault;
        msg!("Vault created for mint {}", vault.deposit_mint);
        Ok(())
    }

    /// Неделя 8: создаёт «карточку» пользователя.
    pub fn open_user_position(ctx: Context<OpenUserPosition>) -> Result<()> {
        let position = &mut ctx.accounts.position;
        position.vault = ctx.accounts.vault.key();
        position.owner = ctx.accounts.user.key();
        position.deposited = 0;
        position.bump = ctx.bumps.position;
        msg!("Position opened for {}", position.owner);
        Ok(())
    }

    /// Неделя 9: пользователь вносит токены и получает share-токены.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);

        let share_supply = ctx.accounts.share_mint.supply;
        let total_assets = ctx.accounts.vault_token.amount;

        let shares: u64 = if share_supply == 0 {
            amount
        } else {
            require!(total_assets > 0, VaultError::EmptyVault);
            let s = (amount as u128)
                .checked_mul(share_supply as u128)
                .ok_or(VaultError::MathOverflow)?
                / (total_assets as u128);
            u64::try_from(s).map_err(|_| VaultError::MathOverflow)?
        };
        require!(shares > 0, VaultError::ZeroShares);

        // CPI: перевод токенов пользователя в хранилище (подписывает пользователь)
        let transfer_accounts = TransferChecked {
            from: ctx.accounts.user_token.to_account_info(),
            mint: ctx.accounts.deposit_mint.to_account_info(),
            to: ctx.accounts.vault_token.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let transfer_ctx =
            CpiContext::new(ctx.accounts.token_program.key(), transfer_accounts);
        token_interface::transfer_checked(
            transfer_ctx,
            amount,
            ctx.accounts.deposit_mint.decimals,
        )?;

        // CPI: выпуск share-токенов (подписывает программа от имени PDA vault)
        let deposit_mint_key = ctx.accounts.vault.deposit_mint;
        let bump = [ctx.accounts.vault.bump];
        let signer_seeds: &[&[&[u8]]] = &[&[b"vault", deposit_mint_key.as_ref(), &bump]];

        let mint_accounts = MintTo {
            mint: ctx.accounts.share_mint.to_account_info(),
            to: ctx.accounts.user_share.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        };
        let mint_ctx = CpiContext::new(ctx.accounts.token_program.key(), mint_accounts)
            .with_signer(signer_seeds);
        token_interface::mint_to(mint_ctx, shares)?;

        let vault = &mut ctx.accounts.vault;
        vault.total_deposited = vault
            .total_deposited
            .checked_add(amount)
            .ok_or(VaultError::MathOverflow)?;

        let position = &mut ctx.accounts.position;
        position.deposited = position
            .deposited
            .checked_add(amount)
            .ok_or(VaultError::MathOverflow)?;

        msg!("Deposited {} tokens, minted {} shares", amount, shares);
        Ok(())
    }

    /// Неделя 10: пользователь возвращает share-токены и забирает свою долю.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(shares > 0, VaultError::ZeroAmount);
        let user_shares = ctx.accounts.user_share.amount;
        require!(shares <= user_shares, VaultError::InsufficientShares);

        let share_supply = ctx.accounts.share_mint.supply;
        let total_assets = ctx.accounts.vault_token.amount;

        // Сколько токенов положено: shares / все share * всё, что лежит в vault
        let amount_out = u64::try_from(
            (shares as u128)
                .checked_mul(total_assets as u128)
                .ok_or(VaultError::MathOverflow)?
                / (share_supply as u128),
        )
        .map_err(|_| VaultError::MathOverflow)?;
        require!(amount_out > 0, VaultError::NothingToWithdraw);

        // Какая часть первоначального вклада выводится (для учёта)
        let principal_out = u64::try_from(
            (ctx.accounts.position.deposited as u128)
                .checked_mul(shares as u128)
                .ok_or(VaultError::MathOverflow)?
                / (user_shares as u128),
        )
        .map_err(|_| VaultError::MathOverflow)?;

        // CPI: сжигаем share-токены (подписывает пользователь)
        let burn_accounts = Burn {
            mint: ctx.accounts.share_mint.to_account_info(),
            from: ctx.accounts.user_share.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let burn_ctx = CpiContext::new(ctx.accounts.token_program.key(), burn_accounts);
        token_interface::burn(burn_ctx, shares)?;

        // CPI: токены из хранилища пользователю (подписывает программа от имени PDA vault)
        let deposit_mint_key = ctx.accounts.vault.deposit_mint;
        let bump = [ctx.accounts.vault.bump];
        let signer_seeds: &[&[&[u8]]] = &[&[b"vault", deposit_mint_key.as_ref(), &bump]];
        let transfer_accounts = TransferChecked {
            from: ctx.accounts.vault_token.to_account_info(),
            mint: ctx.accounts.deposit_mint.to_account_info(),
            to: ctx.accounts.user_token.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        };
        let transfer_ctx = CpiContext::new(ctx.accounts.token_program.key(), transfer_accounts)
            .with_signer(signer_seeds);
        token_interface::transfer_checked(
            transfer_ctx,
            amount_out,
            ctx.accounts.deposit_mint.decimals,
        )?;

        let vault = &mut ctx.accounts.vault;
        vault.total_deposited = vault
            .total_deposited
            .checked_sub(principal_out)
            .ok_or(VaultError::MathOverflow)?;
        let position = &mut ctx.accounts.position;
        position.deposited = position
            .deposited
            .checked_sub(principal_out)
            .ok_or(VaultError::MathOverflow)?;

        msg!("Burned {} shares, withdrew {} tokens", shares, amount_out);
        Ok(())
    }

    /// Неделя 12: администратор создаёт хранилище для второго токена (токен B).
    pub fn init_token_b(ctx: Context<InitTokenB>) -> Result<()> {
        msg!("Vault token B created: {}", ctx.accounts.vault_token_b.key());
        Ok(())
    }

    /// Неделя 12: обмен через CLMM-пул Raydium (CPI). Только администратор.
    /// a_to_b = true: токен A -> токен B, false: токен B -> токен A.
    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64,
        sqrt_price_limit_x64: u128,
        a_to_b: bool,
    ) -> Result<()> {
        require!(amount_in > 0, VaultError::ZeroAmount);

        let in_balance = if a_to_b {
            ctx.accounts.vault_token.amount
        } else {
            ctx.accounts.vault_token_b.amount
        };
        require!(in_balance >= amount_in, VaultError::InsufficientVaultBalance);
        let out_before = if a_to_b {
            ctx.accounts.vault_token_b.amount
        } else {
            ctx.accounts.vault_token.amount
        };

        // Подпись PDA vault: seeds ["vault", deposit_mint, bump]
        let deposit_mint_key = ctx.accounts.vault.deposit_mint;
        let bump = [ctx.accounts.vault.bump];
        let signer_seeds: &[&[&[u8]]] = &[&[b"vault", deposit_mint_key.as_ref(), &bump]];

        {
            let a = &ctx.accounts;
            let (input_token, output_token, input_mint, output_mint) = if a_to_b {
                (&a.vault_token, &a.vault_token_b, &a.deposit_mint, &a.token_b_mint)
            } else {
                (&a.vault_token_b, &a.vault_token, &a.token_b_mint, &a.deposit_mint)
            };

            // Данные инструкции swap_v2 формирует сама программа (41 байт)
            let mut data: Vec<u8> = Vec::with_capacity(41);
            data.extend_from_slice(&SWAP_V2_DISCRIMINATOR);
            data.extend_from_slice(&amount_in.to_le_bytes());
            data.extend_from_slice(&min_amount_out.to_le_bytes());
            data.extend_from_slice(&sqrt_price_limit_x64.to_le_bytes());
            data.push(1u8); // is_base_input = true: точная сумма на входе

            // Порядок аккаунтов swap_v2 (проверен по инструкции, которую собирает SDK Raydium)
            let metas = vec![
                AccountMeta::new_readonly(a.vault.key(), true), // payer: PDA vault
                AccountMeta::new_readonly(a.amm_config.key(), false),
                AccountMeta::new(a.pool_state.key(), false),
                AccountMeta::new(input_token.key(), false),
                AccountMeta::new(output_token.key(), false),
                AccountMeta::new(a.pool_input_vault.key(), false),
                AccountMeta::new(a.pool_output_vault.key(), false),
                AccountMeta::new(a.observation_state.key(), false),
                AccountMeta::new_readonly(a.token_program.key(), false),
                AccountMeta::new_readonly(a.token_program_2022.key(), false),
                AccountMeta::new_readonly(a.memo_program.key(), false),
                AccountMeta::new_readonly(input_mint.key(), false),
                AccountMeta::new_readonly(output_mint.key(), false),
                AccountMeta::new(a.bitmap_extension.key(), false),
                AccountMeta::new(a.tick_array_0.key(), false),
                AccountMeta::new(a.tick_array_1.key(), false),
            ];
            let ix = Instruction {
                program_id: CLMM_PROGRAM_ID,
                accounts: metas,
                data,
            };

            let infos = vec![
                a.vault.to_account_info(),
                a.amm_config.to_account_info(),
                a.pool_state.to_account_info(),
                input_token.to_account_info(),
                output_token.to_account_info(),
                a.pool_input_vault.to_account_info(),
                a.pool_output_vault.to_account_info(),
                a.observation_state.to_account_info(),
                a.token_program.to_account_info(),
                a.token_program_2022.to_account_info(),
                a.memo_program.to_account_info(),
                input_mint.to_account_info(),
                output_mint.to_account_info(),
                a.bitmap_extension.to_account_info(),
                a.tick_array_0.to_account_info(),
                a.tick_array_1.to_account_info(),
                a.clmm_program.to_account_info(),
            ];
            invoke_signed(&ix, &infos, signer_seeds)?;
        }

        // Сколько реально получили, и вторая защита от проскальзывания
        let out_after = if a_to_b {
            ctx.accounts.vault_token_b.reload()?;
            ctx.accounts.vault_token_b.amount
        } else {
            ctx.accounts.vault_token.reload()?;
            ctx.accounts.vault_token.amount
        };
        let received = out_after
            .checked_sub(out_before)
            .ok_or(VaultError::MathOverflow)?;
        require!(received >= min_amount_out, VaultError::SlippageExceeded);

        msg!("Swapped {} in, received {} out", amount_in, received);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mint::token_program = token_program)]
    pub deposit_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = 8 + Vault::INIT_SPACE,
        seeds = [b"vault", deposit_mint.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,

    #[account(
        init,
        payer = admin,
        seeds = [b"share_mint", vault.key().as_ref()],
        bump,
        mint::decimals = deposit_mint.decimals,
        mint::authority = vault,
        mint::token_program = token_program
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        seeds = [b"vault_token", vault.key().as_ref()],
        bump,
        token::mint = deposit_mint,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct OpenUserPosition<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,

    #[account(
        init,
        payer = user,
        space = 8 + UserPosition::INIT_SPACE,
        seeds = [b"position", vault.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub position: Account<'info, UserPosition>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump,
        has_one = deposit_mint,
        has_one = share_mint,
        has_one = vault_token
    )]
    pub vault: Box<Account<'info, Vault>>,

    pub deposit_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub share_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = deposit_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = share_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_share: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"position", vault.key().as_ref(), user.key().as_ref()],
        bump = position.bump
    )]
    pub position: Box<Account<'info, UserPosition>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump,
        has_one = deposit_mint,
        has_one = share_mint,
        has_one = vault_token
    )]
    pub vault: Box<Account<'info, Vault>>,

    pub deposit_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub share_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = deposit_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = share_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_share: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"position", vault.key().as_ref(), user.key().as_ref()],
        bump = position.bump
    )]
    pub position: Box<Account<'info, UserPosition>>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct InitTokenB<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump,
        has_one = admin @ VaultError::NotAdmin
    )]
    pub vault: Box<Account<'info, Vault>>,

    #[account(
        constraint = token_b_mint.key() != vault.deposit_mint @ VaultError::SameMint,
        mint::token_program = token_program
    )]
    pub token_b_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = admin,
        seeds = [b"vault_token_b", vault.key().as_ref()],
        bump,
        token::mint = token_b_mint,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump,
        has_one = admin @ VaultError::NotAdmin,
        has_one = deposit_mint,
        has_one = vault_token
    )]
    pub vault: Box<Account<'info, Vault>>,

    pub deposit_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_b_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"vault_token_b", vault.key().as_ref()],
        bump,
        token::mint = token_b_mint,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: только программа Raydium CLMM (адрес зашит в константе)
    #[account(address = CLMM_PROGRAM_ID)]
    pub clmm_program: UncheckedAccount<'info>,

    /// CHECK: проверяется программой Raydium
    pub amm_config: UncheckedAccount<'info>,
    /// CHECK: проверяется программой Raydium
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,
    /// CHECK: хранилище пула для токена входа, проверяется программой Raydium
    #[account(mut)]
    pub pool_input_vault: UncheckedAccount<'info>,
    /// CHECK: хранилище пула для токена выхода, проверяется программой Raydium
    #[account(mut)]
    pub pool_output_vault: UncheckedAccount<'info>,
    /// CHECK: проверяется программой Raydium
    #[account(mut)]
    pub observation_state: UncheckedAccount<'info>,
    /// CHECK: Token-2022, проверяется программой Raydium
    pub token_program_2022: UncheckedAccount<'info>,
    /// CHECK: Memo, проверяется программой Raydium
    pub memo_program: UncheckedAccount<'info>,
    /// CHECK: расширение bitmap пула, проверяется программой Raydium
    #[account(mut)]
    pub bitmap_extension: UncheckedAccount<'info>,
    /// CHECK: массив тиков, проверяется программой Raydium
    #[account(mut)]
    pub tick_array_0: UncheckedAccount<'info>,
    /// CHECK: массив тиков, проверяется программой Raydium
    #[account(mut)]
    pub tick_array_1: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub admin: Pubkey,
    pub deposit_mint: Pubkey,
    pub share_mint: Pubkey,
    pub vault_token: Pubkey,
    pub total_deposited: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UserPosition {
    pub vault: Pubkey,
    pub owner: Pubkey,
    pub deposited: u64,
    pub bump: u8,
}

#[error_code]
pub enum VaultError {
    #[msg("Deposit amount must be greater than zero")]
    ZeroAmount,
    #[msg("Deposit is too small: zero shares would be minted")]
    ZeroShares,
    #[msg("Vault has shares but no assets")]
    EmptyVault,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Not enough share tokens")]
    InsufficientShares,
    #[msg("Withdrawal amount is zero")]
    NothingToWithdraw,
    #[msg("Only the vault admin can do this")]
    NotAdmin,
    #[msg("Token B must differ from the deposit token")]
    SameMint,
    #[msg("Not enough tokens in the vault")]
    InsufficientVaultBalance,
    #[msg("Swap output is below the minimum (slippage)")]
    SlippageExceeded,
}
