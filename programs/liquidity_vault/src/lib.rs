use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{
    self, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("C9pH3cWmgQihD2xPQgWxsb7QyWkNpXQNPxsCBkVJrQM1");

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
        // 1. Нельзя вносить ноль
        require!(amount > 0, VaultError::ZeroAmount);

        // 2. Сколько share-токенов положено
        let share_supply = ctx.accounts.share_mint.supply;
        let total_assets = ctx.accounts.vault_token.amount;

        let shares: u64 = if share_supply == 0 {
            // первый вкладчик: share-токенов столько же, сколько внёс
            amount
        } else {
            // вклад * все share-токены / всё, что лежит в vault
            require!(total_assets > 0, VaultError::EmptyVault);
            let s = (amount as u128)
                .checked_mul(share_supply as u128)
                .ok_or(VaultError::MathOverflow)?
                / (total_assets as u128);
            u64::try_from(s).map_err(|_| VaultError::MathOverflow)?
        };
        require!(shares > 0, VaultError::ZeroShares);

        // 3. CPI: перевод токенов пользователя в хранилище (подписывает пользователь)
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

        // 4. CPI: выпуск share-токенов (подписывает программа от имени PDA vault)
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

        // 5. Обновляем учёт
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

    /// Токены пользователя, которые он вносит
    #[account(
        mut,
        token::mint = deposit_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Кошелёк пользователя для share-токенов (создаётся при первом депозите)
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
}
