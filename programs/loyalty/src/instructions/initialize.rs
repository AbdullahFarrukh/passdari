use anchor_lang::prelude::*;
use crate::state::Counter;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + Counter::INIT_SPACE,
        seeds = [b"counter", owner.key().as_ref()],
        bump
    )]
    pub counter: Account<'info, Counter>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Initialize>) -> Result<()> {
    let counter = &mut ctx.accounts.counter;
    counter.owner = ctx.accounts.owner.key();
    counter.count = 0;
    counter.bump = ctx.bumps.counter;
    msg!("Counter created for {:?}", counter.owner);
    Ok(())
}