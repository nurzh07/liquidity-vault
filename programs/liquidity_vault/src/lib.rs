pub mod instructions;

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
    184, 152, 151, 52, 252, 179, 140, 145, 104, 216, 83, 199, 83, 182, 184, 164, 54, 16, 205, 211,
    37, 175, 187, 199, 47, 212, 21, 54, 219, 205, 194, 88,
]);

/// Идентификатор инструкции swap_v2 в CLMM (первые 8 байт sha256 от "global:swap_v2")
pub const SWAP_V2_DISCRIMINATOR: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];

fn ensure_vault_idle(position: &AccountInfo, token_b: &AccountInfo) -> Result<()> {
    if !position.data_is_empty() {
        require_keys_eq!(*position.owner, crate::ID, VaultError::InvalidClmmAccount);

        let data = position.try_borrow_data()?;
        let mut bytes: &[u8] = &data;

        let state = instructions::clmm::ClmmPosition::try_deserialize(&mut bytes)?;

        require!(!state.active, VaultError::ClmmPositionActive);
    }

    if !token_b.data_is_empty() {
        require!(
            token_b.owner == &anchor_spl::token::ID || token_b.owner == &anchor_spl::token_2022::ID,
            VaultError::InvalidClmmAccount
        );

        let data = token_b.try_borrow_data()?;
        let mut bytes: &[u8] = &data;
        let account = TokenAccount::try_deserialize(&mut bytes)?;

        require!(account.amount == 0, VaultError::TokenBNotEmpty);
    }

    Ok(())
}

#[program]
pub mod liquidity_vault {
    use super::*;

    pub fn collect_fees(ctx: Context<ManageClmmPosition>) -> Result<()> {
        let a = ctx.accounts;

        require!(a.clmm_position.active, VaultError::ClmmNotActive);

        decrease_clmm_position(a, 0, 0, 0)?;

        msg!("CLMM fees collected");

        Ok(())
    }

    pub fn close_clmm_position(
        ctx: Context<ManageClmmPosition>,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        use crate::instructions::clmm::*;

        let a = ctx.accounts;

        require!(a.clmm_position.active, VaultError::ClmmNotActive);

        let liquidity = a.clmm_position.liquidity;

        decrease_clmm_position(a, liquidity, amount_0_min, amount_1_min)?;

        let mint = a.vault.deposit_mint;
        let bump = [a.vault.bump];

        let seeds: &[&[&[u8]]] = &[&[b"vault", mint.as_ref(), &bump]];

        let infos = vec![
            a.vault.to_account_info(),
            a.nft_mint.to_account_info(),
            a.nft_account.to_account_info(),
            a.personal_position.to_account_info(),
            a.system_program.to_account_info(),
            a.token_program.to_account_info(),
        ];

        let metas = vec![
            AccountMeta::new(a.vault.key(), true),
            AccountMeta::new(a.nft_mint.key(), false),
            AccountMeta::new(a.nft_account.key(), false),
            AccountMeta::new(a.personal_position.key(), false),
            AccountMeta::new_readonly(a.system_program.key(), false),
            AccountMeta::new_readonly(a.token_program.key(), false),
        ];

        invoke_clmm(
            a.clmm_program.to_account_info(),
            &infos,
            metas,
            close_position_data(),
            seeds,
        )?;

        a.clmm_position.active = false;
        a.clmm_position.liquidity = 0;

        msg!("CLMM position closed");

        Ok(())
    }

    pub fn open_clmm_position(
        mut ctx: Context<OpenClmmPosition>,
        liquidity: u128,
        amount_0_max: u64,
        amount_1_max: u64,
        tick_lower: i32,
        tick_upper: i32,
        tick_array_lower_start: i32,
        tick_array_upper_start: i32,
    ) -> Result<()> {
        use crate::instructions::clmm::*;

        let a = &mut ctx.accounts;

        require!(!a.clmm_position.active, VaultError::ClmmAlreadyActive);

        require!(liquidity > 0, VaultError::ZeroAmount);

        require!(tick_lower < tick_upper, VaultError::InvalidTickRange);

        let mint = a.vault.deposit_mint;
        let bump = [a.vault.bump];

        let seeds: &[&[&[u8]]] = &[&[b"vault", mint.as_ref(), &bump]];

        let vault_info = a.vault.to_account_info();

        let (token_0, token_1) = if a.vault_token.mint == a.mint_0.key() {
            (&a.vault_token, &a.vault_token_b)
        } else {
            (&a.vault_token_b, &a.vault_token)
        };

        require_keys_eq!(token_0.mint, a.mint_0.key());
        require_keys_eq!(token_1.mint, a.mint_1.key());

        require!(
            token_0.amount >= amount_0_max,
            VaultError::InsufficientVaultBalance
        );

        require!(
            token_1.amount >= amount_1_max,
            VaultError::InsufficientVaultBalance
        );

        let before_0 = token_0.amount;
        let before_1 = token_1.amount;

        let token_0_info = token_0.to_account_info();
        let token_1_info = token_1.to_account_info();

        approve_tokens(
            a.token_program.to_account_info(),
            token_0_info.clone(),
            a.admin.to_account_info(),
            vault_info.clone(),
            amount_0_max,
            seeds,
        )?;

        approve_tokens(
            a.token_program.to_account_info(),
            token_1_info.clone(),
            a.admin.to_account_info(),
            vault_info.clone(),
            amount_1_max,
            seeds,
        )?;

        let infos = vec![
            a.admin.to_account_info(),
            vault_info.clone(),
            a.nft_mint.to_account_info(),
            a.nft_account.to_account_info(),
            a.metadata_account.to_account_info(),
            a.pool_state.to_account_info(),
            a.protocol_position.to_account_info(),
            a.tick_array_lower.to_account_info(),
            a.tick_array_upper.to_account_info(),
            a.personal_position.to_account_info(),
            token_0_info.clone(),
            token_1_info.clone(),
            a.pool_vault_0.to_account_info(),
            a.pool_vault_1.to_account_info(),
            a.rent.to_account_info(),
            a.system_program.to_account_info(),
            a.token_program.to_account_info(),
            a.associated_token_program.to_account_info(),
            a.metadata_program.to_account_info(),
            a.token_program_2022.to_account_info(),
            a.mint_0.to_account_info(),
            a.mint_1.to_account_info(),
            a.bitmap_extension.to_account_info(),
        ];

        let writable = [0usize, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 22];

        let metas = infos
            .iter()
            .enumerate()
            .map(|(i, account)| {
                let signer = i == 0 || i == 2;

                if writable.contains(&i) {
                    AccountMeta::new(account.key(), signer)
                } else {
                    AccountMeta::new_readonly(account.key(), signer)
                }
            })
            .collect();

        let data = open_position_data(
            liquidity,
            amount_0_max,
            amount_1_max,
            tick_lower,
            tick_upper,
            tick_array_lower_start,
            tick_array_upper_start,
        );

        invoke_clmm(a.clmm_program.to_account_info(), &infos, metas, data, seeds)?;

        revoke_tokens(
            a.token_program.to_account_info(),
            token_0_info.clone(),
            vault_info.clone(),
            seeds,
        )?;

        revoke_tokens(
            a.token_program.to_account_info(),
            token_1_info.clone(),
            vault_info,
            seeds,
        )?;

        let after_0 =
            TokenAccount::try_deserialize(&mut &token_0_info.try_borrow_data()?[..])?.amount;

        let after_1 =
            TokenAccount::try_deserialize(&mut &token_1_info.try_borrow_data()?[..])?.amount;

        let spent_0 = before_0
            .checked_sub(after_0)
            .ok_or(VaultError::MathOverflow)?;

        let spent_1 = before_1
            .checked_sub(after_1)
            .ok_or(VaultError::MathOverflow)?;

        require!(
            spent_0 <= amount_0_max && spent_1 <= amount_1_max,
            VaultError::SlippageExceeded
        );

        let position = &mut a.clmm_position;

        position.vault = a.vault.key();
        position.pool = a.pool_state.key();
        position.nft_mint = a.nft_mint.key();
        position.personal_position = a.personal_position.key();

        position.tick_lower = tick_lower;
        position.tick_upper = tick_upper;
        position.liquidity = liquidity;
        position.active = true;
        position.bump = ctx.bumps.clmm_position;

        msg!("CLMM position opened");
        msg!("Liquidity: {}", liquidity);
        msg!("Token 0 spent: {}", spent_0);
        msg!("Token 1 spent: {}", spent_1);

        Ok(())
    }

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
        ensure_vault_idle(
            &ctx.accounts.clmm_position.to_account_info(),
            &ctx.accounts.vault_token_b.to_account_info(),
        )?;

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
        let transfer_ctx = CpiContext::new(ctx.accounts.token_program.key(), transfer_accounts);
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
        ensure_vault_idle(
            &ctx.accounts.clmm_position.to_account_info(),
            &ctx.accounts.vault_token_b.to_account_info(),
        )?;

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
        msg!(
            "Vault token B created: {}",
            ctx.accounts.vault_token_b.key()
        );
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
        require!(
            in_balance >= amount_in,
            VaultError::InsufficientVaultBalance
        );
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
                (
                    &a.vault_token,
                    &a.vault_token_b,
                    &a.deposit_mint,
                    &a.token_b_mint,
                )
            } else {
                (
                    &a.vault_token_b,
                    &a.vault_token,
                    &a.token_b_mint,
                    &a.deposit_mint,
                )
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

    /// CHECK: Canonical CLMM position PDA.
    #[account(
        seeds = [b"clmm_position", vault.key().as_ref()],
        bump
    )]
    pub clmm_position: UncheckedAccount<'info>,

    /// CHECK: Canonical token B vault PDA.
    #[account(
        seeds = [b"vault_token_b", vault.key().as_ref()],
        bump
    )]
    pub vault_token_b: UncheckedAccount<'info>,

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

    /// CHECK: Canonical CLMM position PDA.
    #[account(
        seeds = [b"clmm_position", vault.key().as_ref()],
        bump
    )]
    pub clmm_position: UncheckedAccount<'info>,

    /// CHECK: Canonical token B vault PDA.
    #[account(
        seeds = [b"vault_token_b", vault.key().as_ref()],
        bump
    )]
    pub vault_token_b: UncheckedAccount<'info>,

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
    #[msg("CLMM position is already active")]
    ClmmAlreadyActive,

    #[msg("CLMM position is not active")]
    ClmmNotActive,

    #[msg("Invalid CLMM tick range")]
    InvalidTickRange,

    #[msg("Invalid CLMM account")]
    InvalidClmmAccount,

    #[msg("Close CLMM position before deposit or withdrawal")]
    ClmmPositionActive,

    #[msg("Convert token B back to token A first")]
    TokenBNotEmpty,

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

#[derive(Accounts)]
pub struct OpenClmmPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump,
        has_one = admin @ VaultError::NotAdmin,
        has_one = vault_token
    )]
    pub vault: Box<Account<'info, Vault>>,

    #[account(
        init_if_needed,
        payer = admin,
        space = 8 + instructions::clmm::ClmmPosition::INIT_SPACE,
        seeds = [b"clmm_position", vault.key().as_ref()],
        bump
    )]
    pub clmm_position: Box<Account<'info, instructions::clmm::ClmmPosition>>,

    #[account(
        mut,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"vault_token_b", vault.key().as_ref()],
        bump,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub nft_mint: Signer<'info>,

    /// CHECK: Raydium validates NFT ATA.
    #[account(mut)]
    pub nft_account: UncheckedAccount<'info>,

    /// CHECK: Raydium metadata account.
    #[account(mut)]
    pub metadata_account: UncheckedAccount<'info>,

    /// CHECK: Raydium validates pool state.
    #[account(mut, owner = CLMM_PROGRAM_ID)]
    pub pool_state: UncheckedAccount<'info>,

    /// CHECK: Deprecated Raydium protocol position.
    pub protocol_position: UncheckedAccount<'info>,

    /// CHECK: Validated by Raydium.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: Validated by Raydium.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    /// CHECK: Initialized by Raydium.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    #[account(mut, token::mint = mint_0)]
    pub pool_vault_0: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = mint_1)]
    pub pool_vault_1: Box<InterfaceAccount<'info, TokenAccount>>,

    pub mint_0: Box<InterfaceAccount<'info, Mint>>,
    pub mint_1: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: Validated by Raydium.
    #[account(mut)]
    pub bitmap_extension: UncheckedAccount<'info>,

    /// CHECK: Fixed Raydium Devnet program.
    #[account(address = CLMM_PROGRAM_ID, executable)]
    pub clmm_program: UncheckedAccount<'info>,

    /// CHECK: Fixed Metaplex program.
    #[account(
        address = Pubkey::from_str_const(
            "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
        ),
        executable
    )]
    pub metadata_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, anchor_spl::token::Token>,

    pub token_program_2022: Program<'info, anchor_spl::token_2022::Token2022>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

fn decrease_clmm_position<'info>(
    a: &mut ManageClmmPosition<'info>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    use crate::instructions::clmm::*;

    let mint = a.vault.deposit_mint;
    let bump = [a.vault.bump];

    let seeds: &[&[&[u8]]] = &[&[b"vault", mint.as_ref(), &bump]];

    let (token_0, token_1) = if a.vault_token.mint == a.mint_0.key() {
        (&a.vault_token, &a.vault_token_b)
    } else {
        (&a.vault_token_b, &a.vault_token)
    };

    require_keys_eq!(token_0.mint, a.mint_0.key());
    require_keys_eq!(token_1.mint, a.mint_1.key());

    let before_0 = token_0.amount;
    let before_1 = token_1.amount;

    let token_0_info = token_0.to_account_info();
    let token_1_info = token_1.to_account_info();

    let infos = vec![
        a.vault.to_account_info(),
        a.nft_account.to_account_info(),
        a.personal_position.to_account_info(),
        a.pool_state.to_account_info(),
        a.protocol_position.to_account_info(),
        a.pool_vault_0.to_account_info(),
        a.pool_vault_1.to_account_info(),
        a.tick_array_lower.to_account_info(),
        a.tick_array_upper.to_account_info(),
        token_0_info.clone(),
        token_1_info.clone(),
        a.token_program.to_account_info(),
        a.token_program_2022.to_account_info(),
        a.memo_program.to_account_info(),
        a.mint_0.to_account_info(),
        a.mint_1.to_account_info(),
        a.bitmap_extension.to_account_info(),
    ];

    let writable = [2usize, 3, 5, 6, 7, 8, 9, 10, 16];

    let metas = infos
        .iter()
        .enumerate()
        .map(|(i, account)| {
            if writable.contains(&i) {
                AccountMeta::new(account.key(), i == 0)
            } else {
                AccountMeta::new_readonly(account.key(), i == 0)
            }
        })
        .collect();

    invoke_clmm(
        a.clmm_program.to_account_info(),
        &infos,
        metas,
        decrease_liquidity_data(liquidity, amount_0_min, amount_1_min),
        seeds,
    )?;

    let after_0 = TokenAccount::try_deserialize(&mut &token_0_info.try_borrow_data()?[..])?.amount;

    let after_1 = TokenAccount::try_deserialize(&mut &token_1_info.try_borrow_data()?[..])?.amount;

    let received_0 = after_0
        .checked_sub(before_0)
        .ok_or(VaultError::MathOverflow)?;

    let received_1 = after_1
        .checked_sub(before_1)
        .ok_or(VaultError::MathOverflow)?;

    require!(
        received_0 >= amount_0_min && received_1 >= amount_1_min,
        VaultError::SlippageExceeded
    );

    msg!("Token 0 received: {}", received_0);
    msg!("Token 1 received: {}", received_1);

    Ok(())
}

#[derive(Accounts)]
pub struct ManageClmmPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault.deposit_mint.as_ref()],
        bump = vault.bump,
        has_one = admin @ VaultError::NotAdmin,
        has_one = vault_token
    )]
    pub vault: Box<Account<'info, Vault>>,

    #[account(
        mut,
        seeds = [b"clmm_position", vault.key().as_ref()],
        bump = clmm_position.bump,
        has_one = vault,
        has_one = nft_mint,
        has_one = personal_position,
        constraint = clmm_position.pool == pool_state.key()
            @ VaultError::InvalidClmmAccount
    )]
    pub clmm_position: Box<Account<'info, instructions::clmm::ClmmPosition>>,

    #[account(
        mut,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"vault_token_b", vault.key().as_ref()],
        bump,
        token::authority = vault,
        token::token_program = token_program
    )]
    pub vault_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub nft_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        token::mint = nft_mint,
        token::authority = vault,
        token::token_program = token_program,
        constraint = nft_account.amount == 1
            @ VaultError::InvalidClmmAccount
    )]
    pub nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Stored Raydium personal position.
    #[account(mut, owner = CLMM_PROGRAM_ID)]
    pub personal_position: UncheckedAccount<'info>,

    /// CHECK: Stored Raydium pool.
    #[account(mut, owner = CLMM_PROGRAM_ID)]
    pub pool_state: UncheckedAccount<'info>,

    /// CHECK: Deprecated protocol position.
    pub protocol_position: UncheckedAccount<'info>,

    /// CHECK: Validated by Raydium.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: Validated by Raydium.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    #[account(mut, token::mint = mint_0)]
    pub pool_vault_0: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = mint_1)]
    pub pool_vault_1: Box<InterfaceAccount<'info, TokenAccount>>,

    pub mint_0: Box<InterfaceAccount<'info, Mint>>,
    pub mint_1: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: Validated by Raydium.
    #[account(mut)]
    pub bitmap_extension: UncheckedAccount<'info>,

    /// CHECK: Fixed Raydium program.
    #[account(address = CLMM_PROGRAM_ID, executable)]
    pub clmm_program: UncheckedAccount<'info>,

    /// CHECK: Fixed SPL Memo program.
    #[account(
        address = Pubkey::from_str_const(
            "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
        ),
        executable
    )]
    pub memo_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, anchor_spl::token::Token>,

    pub token_program_2022: Program<'info, anchor_spl::token_2022::Token2022>,

    pub system_program: Program<'info, System>,
}
