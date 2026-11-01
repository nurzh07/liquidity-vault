use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

declare_id!("C9pH3cWmgQihD2xPQgWxsb7QyWkNpXQNPxsCBkVJrQM1");

#[program]
pub mod liquidity_vault {
    use super::*;

    /// Создаёт vault: настройки, share-токен и хранилище токенов.
    /// Вызывается один раз для каждого депозитного токена.
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

    /// Создаёт «карточку» пользователя (PDA от адреса пользователя).
    pub fn open_user_position(ctx: Context<OpenUserPosition>) -> Result<()> {
        let position = &mut ctx.accounts.position;
        position.vault = ctx.accounts.vault.key();
        position.owner = ctx.accounts.user.key();
        position.deposited = 0;
        position.bump = ctx.bumps.position;
        msg!("Position opened for {}", position.owner);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Токен, который принимает vault
    #[account(mint::token_program = token_program)]
    pub deposit_mint: InterfaceAccount<'info, Mint>,

    /// Настройки vault (PDA)
    #[account(
        init,
        payer = admin,
        space = 8 + Vault::INIT_SPACE,
        seeds = [b"vault", deposit_mint.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,

    /// Share-токен: выпускать его может только PDA vault (то есть программа)
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

    /// Хранилище токенов: владелец (authority) = PDA vault
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
