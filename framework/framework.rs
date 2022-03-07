use anchor_client::{
    anchor_lang::System,
    solana_sdk::{
        instruction::Instruction, program_pack::Pack, system_instruction::transfer,
        transaction::Transaction,
    },
    Program,
};
use anchor_lang::{solana_program, Id};
use anchor_spl::associated_token::get_associated_token_address;
use jet_auth::accounts::{Authenticate, CreateUserAuthentication};
use jet_staking::{
    accounts::{AddStake, InitPool, InitStakeAccount},
    Amount,
};
use poc_framework::{
    solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer},
    solana_transaction_status::EncodedConfirmedTransaction,
    Environment, LocalEnvironment,
};
use std::{error::Error, path::Path, rc::Rc};

pub struct Framework {
    pub env: LocalEnvironment,
    pub victim: Keypair,
    pub attacker: Keypair,
    pub pool_authority: Keypair,
    auth_program_client: Program,
    stake_program_client: Program,
    rewards_program_client: Program,
    pub vault_token_mint: Keypair,
    pub seed: String,
    pub nop_program_pubkey: Pubkey,
    tx_nonce: u64,
}

impl Framework {
    pub fn process_tx_result(result: EncodedConfirmedTransaction) {
        let meta = result.transaction.meta.unwrap();

        if meta.status.is_err() {
            for line in meta.log_messages.unwrap() {
                println!("{}", line);
            }
        }
    }
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut env_builder = LocalEnvironment::builder();

        let base_path = "./build/jet-governance/target/bpfel-unknown-unknown/release".to_owned();
        env_builder.add_program(
            jet_auth::id(),
            Path::new(&(base_path.clone() + "/jet_auth.so")),
        );
        env_builder.add_program(
            jet_staking::id(),
            Path::new(&(base_path.clone() + "/jet_staking.so")),
        );
        env_builder.add_program(
            jet_rewards::id(),
            Path::new(&(base_path + "/jet_rewards.so")),
        );

        let mut env = env_builder.build();

        let attacker = Keypair::new();
        let victim = Keypair::new();
        let pool_authority = Keypair::new();

        let rpc = "https://fake.local".to_owned();
        let wss = rpc.replace("https", "wss");
        let connection = anchor_client::Client::new(
            anchor_client::Cluster::Custom(rpc, wss),
            Rc::new(clone_keypair(&attacker)),
        );
        let auth_program_client = connection.program(jet_auth::id());
        let stake_program_client = connection.program(jet_staking::id());
        let rewards_program_client = connection.program(jet_rewards::id());

        // deploy nop program
        let nop_program_pubkey = env.deploy_program("../framework/nop.so");

        // fund the accounts
        env.execute_as_transaction(
            &[transfer(
                &env.payer().pubkey(),
                &attacker.pubkey(),
                100000000000,
            )],
            &[&env.payer()],
        );
        env.execute_as_transaction(
            &[transfer(
                &env.payer().pubkey(),
                &victim.pubkey(),
                100000000000,
            )],
            &[&env.payer()],
        );
        env.execute_as_transaction(
            &[transfer(
                &env.payer().pubkey(),
                &pool_authority.pubkey(),
                100000000000,
            )],
            &[&env.payer()],
        );

        // create vault token
        let vault_token_mint = Keypair::new();
        env.create_token_mint(&vault_token_mint, pool_authority.pubkey(), None, 9);

        Ok(Self {
            env,
            victim,
            attacker,
            pool_authority,
            auth_program_client,
            stake_program_client,
            rewards_program_client,
            vault_token_mint,
            seed: "seed".into(),
            nop_program_pubkey,
            tx_nonce: 0,
        })
    }

    pub fn create_user_auth(&mut self, user: &Keypair) -> Result<(), Box<dyn Error>> {
        let (auth, _bump) =
            Pubkey::find_program_address(&[user.pubkey().as_ref()], &self.auth_program_client.id());

        let create_user_auth = CreateUserAuthentication {
            user: user.pubkey(),
            payer: user.pubkey(),
            auth,
            system_program: System::id(),
        };

        let create_user_auth_transaction = Transaction::new_signed_with_payer(
            &self
                .auth_program_client
                .request()
                .accounts(create_user_auth)
                .args(jet_auth::instruction::CreateUserAuth {})
                .instructions()?,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        let create_user_auth_transaction_out =
            self.env.execute_transaction(create_user_auth_transaction);

        Framework::process_tx_result(create_user_auth_transaction_out);

        Ok(())
    }

    pub fn authenticate_user(&mut self, user: &Keypair) -> Result<(), Box<dyn Error>> {
        let (auth, _bump) =
            Pubkey::find_program_address(&[user.pubkey().as_ref()], &self.auth_program_client.id());

        let auth_accounts = Authenticate {
            auth,
            // this isn't checked so it doesn't matter
            authority: self.attacker.pubkey(),
        };
        let auth_user_transaction = Transaction::new_signed_with_payer(
            &self
                .auth_program_client
                .request()
                .accounts(auth_accounts)
                .args(jet_auth::instruction::Authenticate {})
                .instructions()?,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        let auth_user_transaction_out = self.env.execute_transaction(auth_user_transaction);
        Framework::process_tx_result(auth_user_transaction_out);

        Ok(())
    }

    pub fn init_stake_pool(&mut self) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();
        let stake_vote_mint = self.stake_vote_mint_pubkey();
        let (stake_collateral_mint, _bump) = Pubkey::find_program_address(
            &[self.seed.as_bytes(), b"collateral-mint".as_ref()],
            &self.stake_program_client.id(),
        );
        let stake_pool_vault = self.stake_pool_vault_pubkey();

        let init_pool_accounts = InitPool {
            payer: self.pool_authority.pubkey(),
            authority: self.pool_authority.pubkey(),
            token_mint: self.vault_token_mint.pubkey(),
            stake_pool,
            stake_vote_mint,
            stake_collateral_mint,
            stake_pool_vault,
            token_program: spl_token::id(),
            system_program: System::id(),
            rent: solana_program::sysvar::rent::id(),
        };
        let init_pool_transaction = Transaction::new_signed_with_payer(
            &self
                .stake_program_client
                .request()
                .accounts(init_pool_accounts)
                .args(jet_staking::instruction::InitPool {
                    seed: self.seed.clone(),
                    config: jet_staking::instructions::PoolConfig { unbond_period: 0 },
                })
                .instructions()?,
            Some(&self.pool_authority.pubkey()),
            &vec![&self.pool_authority],
            self.env.get_recent_blockhash(),
        );
        let init_pool_transaction_out = self.env.execute_transaction(init_pool_transaction);
        Framework::process_tx_result(init_pool_transaction_out);

        Ok(())
    }

    pub fn stake_pool_pubkey(&self) -> Pubkey {
        let (stake_pool, _bump) =
            Pubkey::find_program_address(&[self.seed.as_bytes()], &self.stake_program_client.id());

        stake_pool
    }

    pub fn stake_pool_vault_pubkey(&self) -> Pubkey {
        let (stake_pool_vault, _bump) = Pubkey::find_program_address(
            &[self.seed.as_bytes(), b"vault".as_ref()],
            &self.stake_program_client.id(),
        );

        stake_pool_vault
    }

    pub fn stake_vote_mint_pubkey(&self) -> Pubkey {
        let (stake_vote_mint, _bump) = Pubkey::find_program_address(
            &[self.seed.as_bytes(), b"vote-mint".as_ref()],
            &self.stake_program_client.id(),
        );

        stake_vote_mint
    }

    pub fn stake_account_pubkey(&self, user: &Keypair) -> Pubkey {
        let stake_pool = self.stake_pool_pubkey();
        let (stake_account, _bump) = Pubkey::find_program_address(
            &[stake_pool.as_ref(), user.pubkey().as_ref()],
            &self.stake_program_client.id(),
        );

        stake_account
    }

    pub fn init_stake_account(&mut self, user: &Keypair) -> Result<(), Box<dyn Error>> {
        let (auth, _bump) =
            Pubkey::find_program_address(&[user.pubkey().as_ref()], &self.auth_program_client.id());

        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let init_stake_account_accounts = InitStakeAccount {
            owner: user.pubkey(),
            auth,
            stake_pool,
            stake_account,
            payer: user.pubkey(),
            system_program: System::id(),
        };
        let init_stake_account = Transaction::new_signed_with_payer(
            &self
                .stake_program_client
                .request()
                .accounts(init_stake_account_accounts)
                .args(jet_staking::instruction::InitStakeAccount {})
                .instructions()?,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(init_stake_account));

        Ok(())
    }

    pub fn add_stake(&mut self, user: &Keypair, amount: u64) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let accounts = AddStake {
            stake_pool,
            stake_account,
            payer: user.pubkey(),
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            payer_token_account: get_associated_token_address(
                &user.pubkey(),
                &self.vault_token_mint.pubkey(),
            ),
            token_program: spl_token::id(),
        };
        let mut instructions = self
            .stake_program_client
            .request()
            .accounts(accounts)
            .args(jet_staking::instruction::AddStake {
                amount: Amount {
                    kind: jet_staking::AmountKind::Tokens,
                    value: amount,
                },
            })
            .instructions()?;
        instructions.push(self.nonce_instruction());
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn unbond_stake_shares(
        &mut self,
        user: &Keypair,
        unbond_seed: u32,
        share_amount: u64,
    ) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let (unbonding_account, _bump) = Pubkey::find_program_address(
            &[stake_account.as_ref(), unbond_seed.to_le_bytes().as_ref()],
            &self.stake_program_client.id(),
        );
        let accounts = jet_staking::accounts::UnbondStake {
            stake_pool,
            stake_account,
            payer: user.pubkey(),
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            owner: user.pubkey(),
            unbonding_account,
            system_program: System::id(),
        };
        let transaction = Transaction::new_signed_with_payer(
            &self
                .stake_program_client
                .request()
                .accounts(accounts)
                .args(jet_staking::instruction::UnbondStake {
                    seed: unbond_seed,
                    amount: Amount {
                        kind: jet_staking::AmountKind::Shares,
                        value: share_amount,
                    },
                })
                .instructions()?,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn cancel_unbond(
        &mut self,
        user: &Keypair,
        unbond_seed: u32,
    ) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let (unbonding_account, _bump) = Pubkey::find_program_address(
            &[stake_account.as_ref(), unbond_seed.to_le_bytes().as_ref()],
            &self.stake_program_client.id(),
        );
        let accounts = jet_staking::accounts::CancelUnbond {
            stake_pool,
            stake_account,
            owner: user.pubkey(),
            unbonding_account,
            receiver: user.pubkey(),
        };
        let mut instructions = self
            .stake_program_client
            .request()
            .accounts(accounts)
            .args(jet_staking::instruction::CancelUnbond {})
            .instructions()?;
        instructions.push(self.nonce_instruction());
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn unbond_stake_tokens(
        &mut self,
        user: &Keypair,
        unbond_seed: u32,
        amount: u64,
    ) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let (unbonding_account, _bump) = Pubkey::find_program_address(
            &[stake_account.as_ref(), unbond_seed.to_le_bytes().as_ref()],
            &self.stake_program_client.id(),
        );
        let accounts = jet_staking::accounts::UnbondStake {
            stake_pool,
            stake_account,
            payer: user.pubkey(),
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            owner: user.pubkey(),
            unbonding_account,
            system_program: System::id(),
        };
        let mut instructions = self
            .stake_program_client
            .request()
            .accounts(accounts)
            .args(jet_staking::instruction::UnbondStake {
                seed: unbond_seed,
                amount: Amount {
                    kind: jet_staking::AmountKind::Tokens,
                    value: amount,
                },
            })
            .instructions()?;
        instructions.push(self.nonce_instruction());
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn withdraw_unbonded_stake(
        &mut self,
        user: &Keypair,
        unbond_seed: u32,
    ) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let (unbonding_account, _bump) = Pubkey::find_program_address(
            &[stake_account.as_ref(), unbond_seed.to_le_bytes().as_ref()],
            &self.stake_program_client.id(),
        );
        let accounts = jet_staking::accounts::WithdrawUnbonded {
            stake_pool,
            stake_account,
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            owner: user.pubkey(),
            unbonding_account,
            closer: user.pubkey(),
            token_receiver: get_associated_token_address(
                &user.pubkey(),
                &self.vault_token_mint.pubkey(),
            ),
            token_program: spl_token::id(),
        };
        let mut instructions = self
            .stake_program_client
            .request()
            .accounts(accounts)
            .args(jet_staking::instruction::WithdrawUnbonded {})
            .instructions()?;
        instructions.push(self.nonce_instruction());
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn withdraw_bonded(&mut self, user: &Keypair, amount: u64) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let accounts = jet_staking::accounts::WithdrawBonded {
            stake_pool,
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            token_receiver: get_associated_token_address(
                &user.pubkey(),
                &self.vault_token_mint.pubkey(),
            ),
            token_program: spl_token::id(),
            authority: self.pool_authority.pubkey(),
        };
        let mut instructions = self
            .stake_program_client
            .request()
            .accounts(accounts)
            .args(jet_staking::instruction::WithdrawBonded { amount })
            .instructions()?;
        instructions.push(self.nonce_instruction());
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&user.pubkey()),
            &vec![user, &self.pool_authority],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn mint_votes(&mut self, user: &Keypair, amount: u64) -> Result<(), Box<dyn Error>> {
        let stake_pool = self.stake_pool_pubkey();

        let stake_account = self.stake_account_pubkey(user);
        let accounts = jet_staking::accounts::MintVotes {
            owner: user.pubkey(),
            stake_vote_mint: self.stake_vote_mint_pubkey(),
            voter_token_account: get_associated_token_address(
                &user.pubkey(),
                &self.stake_vote_mint_pubkey(),
            ),
            stake_pool,
            stake_account,
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            token_program: spl_token::id(),
        };
        let transaction = Transaction::new_signed_with_payer(
            &self
                .stake_program_client
                .request()
                .accounts(accounts)
                .args(jet_staking::instruction::MintVotes {
                    amount: Amount {
                        kind: jet_staking::AmountKind::Tokens,
                        value: amount,
                    },
                })
                .instructions()?,
            Some(&user.pubkey()),
            &vec![user],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn award_pubkey(&self, stake_account: Pubkey, seed: String) -> Pubkey {
        let (award, _bump) = Pubkey::find_program_address(
            &[stake_account.as_ref(), seed.as_bytes()],
            &self.rewards_program_client.id(),
        );

        award
    }

    pub fn distribution_pubkey(&self, seed: String) -> Pubkey {
        let (distribution, _bump) =
            Pubkey::find_program_address(&[seed.as_bytes()], &self.rewards_program_client.id());

        distribution
    }

    pub fn reward_vault_pubkey(&self, account: Pubkey, _seed: String) -> Pubkey {
        let (award_vault, _bump) = Pubkey::find_program_address(
            &[account.as_ref(), b"vault".as_ref()],
            &self.rewards_program_client.id(),
        );

        award_vault
    }

    pub fn create_award(
        &mut self,
        creator: &Keypair,
        receiver: &Keypair,
        begin_at: u64,
        end_at: u64,
        amount: u64,
        seed: String,
    ) -> Result<(), Box<dyn Error>> {
        let stake_account = self.stake_account_pubkey(receiver);
        let award = self.award_pubkey(stake_account, seed.clone());
        let vault = self.reward_vault_pubkey(award, seed.clone());
        println!("award: {}, vault: {}, award seed: {}", award, vault, seed);
        let accounts = jet_rewards::accounts::AwardCreate {
            system_program: System::id(),
            award,
            vault,
            token_mint: self.vault_token_mint.pubkey(),
            token_source: get_associated_token_address(
                &creator.pubkey(),
                &self.vault_token_mint.pubkey(),
            ),
            token_source_authority: creator.pubkey(),
            payer_rent: creator.pubkey(),
            token_program: spl_token::id(),
            rent: solana_program::sysvar::rent::id(),
        };
        let transaction = Transaction::new_signed_with_payer(
            &self
                .rewards_program_client
                .request()
                .accounts(accounts)
                .args(jet_rewards::instruction::AwardCreate {
                    params: jet_rewards::AwardCreateParams {
                        seed,
                        authority: creator.pubkey(),
                        stake_account,
                        amount,
                        begin_at,
                        end_at,
                    },
                })
                .instructions()?,
            Some(&creator.pubkey()),
            &vec![creator],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn release_award(
        &mut self,
        receiver: &Keypair,
        seed: String,
    ) -> Result<(), Box<dyn Error>> {
        let stake_account = self.stake_account_pubkey(receiver);
        let stake_pool = self.stake_pool_pubkey();
        let stake_pool_vault = self.stake_pool_vault_pubkey();
        let award = self.award_pubkey(stake_account, seed.clone());
        let vault = self.reward_vault_pubkey(award, seed);
        let accounts = jet_rewards::accounts::AwardRelease {
            award,
            vault,
            token_program: spl_token::id(),
            stake_account,
            stake_pool,
            stake_pool_vault,
            staking_program: jet_staking::id(),
        };

        let mut instructions = self
            .rewards_program_client
            .request()
            .accounts(accounts)
            .args(jet_rewards::instruction::AwardRelease {})
            .instructions()?;
        instructions.push(self.nonce_instruction());

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&receiver.pubkey()),
            &vec![receiver],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn create_distribution(
        &mut self,
        creator: &Keypair,
        receiver: &Keypair,
        begin_at: u64,
        end_at: u64,
        amount: u64,
        seed: String,
    ) -> Result<(), Box<dyn Error>> {
        let distribution = self.distribution_pubkey(seed.clone());
        let vault = self.reward_vault_pubkey(distribution, seed.clone());
        let accounts = jet_rewards::accounts::DistributionCreate {
            system_program: System::id(),
            distribution,
            vault,
            token_mint: self.vault_token_mint.pubkey(),
            payer_rent: creator.pubkey(),
            token_program: spl_token::id(),
            rent: solana_program::sysvar::rent::id(),
            payer_token_authority: creator.pubkey(),
            payer_token_account: get_associated_token_address(
                &creator.pubkey(),
                &self.vault_token_mint.pubkey(),
            ),
        };
        let transaction = Transaction::new_signed_with_payer(
            &self
                .rewards_program_client
                .request()
                .accounts(accounts)
                .args(jet_rewards::instruction::DistributionCreate {
                    params: jet_rewards::DistributionCreateParams {
                        seed,
                        authority: creator.pubkey(),
                        amount,
                        begin_at,
                        end_at,
                        target_account: get_associated_token_address(
                            &receiver.pubkey(),
                            &self.vault_token_mint.pubkey(),
                        ),
                    },
                })
                .instructions()?,
            Some(&creator.pubkey()),
            &vec![creator],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn release_distribution(
        &mut self,
        receiver: &Keypair,
        seed: String,
    ) -> Result<(), Box<dyn Error>> {
        let distribution = self.distribution_pubkey(seed.clone());
        let vault = self.reward_vault_pubkey(distribution, seed);
        let accounts = jet_rewards::accounts::DistributionRelease {
            distribution,
            vault,
            token_program: spl_token::id(),
            target_account: get_associated_token_address(
                &receiver.pubkey(),
                &self.vault_token_mint.pubkey(),
            ),
        };

        let mut instructions = self
            .rewards_program_client
            .request()
            .accounts(accounts)
            .args(jet_rewards::instruction::DistributionRelease {})
            .instructions()?;
        instructions.push(self.nonce_instruction());

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&receiver.pubkey()),
            &vec![receiver],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn create_airdrop(
        &mut self,
        airdrop: &Keypair,
        expire_at: i64,
    ) -> Result<(), Box<dyn Error>> {
        let accounts = jet_rewards::accounts::AirdropCreate {
            system_program: System::id(),
            token_mint: self.vault_token_mint.pubkey(),
            token_program: spl_token::id(),
            rent: solana_program::sysvar::rent::id(),
            airdrop: airdrop.pubkey(),
            authority: self.pool_authority.pubkey(),
            reward_vault: self.reward_vault_pubkey(airdrop.pubkey(), "".to_string()),
            payer: self.pool_authority.pubkey(),
        };
        let mut instructions = vec![];

        let airdrop_account_size = 8 + std::mem::size_of::<jet_rewards::state::Airdrop>();
        instructions.push(solana_program::system_instruction::create_account(
            &self.pool_authority.pubkey(),
            &airdrop.pubkey(),
            self.env.get_rent_excemption(airdrop_account_size),
            airdrop_account_size as u64,
            &jet_rewards::id(),
        ));
        instructions.extend(
            self.rewards_program_client
                .request()
                .accounts(accounts)
                .args(jet_rewards::instruction::AirdropCreate {
                    params: jet_rewards::AirdropCreateParams {
                        expire_at,
                        stake_pool: self.stake_pool_pubkey(),
                        short_desc: "sdhdfshdfshdfhdfdfhhdf".to_string(),
                        // flags are unused rn
                        flags: 0,
                    },
                })
                .instructions()?,
        );
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.pool_authority.pubkey()),
            &vec![&self.pool_authority, airdrop],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn airdrop_add_recipients(
        &mut self,
        recipients: Vec<jet_rewards::AirdropRecipientParam>,
        airdrop: Pubkey,
        start_index: u64,
    ) -> Result<(), Box<dyn Error>> {
        let accounts = jet_rewards::accounts::AirdropAddRecipients {
            airdrop,
            authority: self.pool_authority.pubkey(),
        };

        let mut instructions = self
            .rewards_program_client
            .request()
            .accounts(accounts)
            .args(jet_rewards::instruction::AirdropAddRecipients {
                params: jet_rewards::AirdropAddRecipientsParams {
                    start_index,
                    recipients,
                },
            })
            .instructions()?;
        instructions.push(self.nonce_instruction());

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.pool_authority.pubkey()),
            &vec![&self.pool_authority],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn airdrop_finalize(&mut self, airdrop: Pubkey) -> Result<(), Box<dyn Error>> {
        let accounts = jet_rewards::accounts::AirdropFinalize {
            airdrop,
            authority: self.pool_authority.pubkey(),
            reward_vault: self.reward_vault_pubkey(airdrop, "".to_string()),
        };

        let mut instructions = self
            .rewards_program_client
            .request()
            .accounts(accounts)
            .args(jet_rewards::instruction::AirdropFinalize {})
            .instructions()?;
        instructions.push(self.nonce_instruction());

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.pool_authority.pubkey()),
            &vec![&self.pool_authority],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }

    pub fn airdrop_claim(
        &mut self,
        recipient: &Keypair,
        airdrop: Pubkey,
    ) -> Result<(), Box<dyn Error>> {
        let accounts = jet_rewards::accounts::AirdropClaim {
            airdrop,
            reward_vault: self.reward_vault_pubkey(airdrop, "".to_string()),
            recipient: recipient.pubkey(),
            // receiver is unused dunno why its there tbh
            receiver: recipient.pubkey(),
            stake_pool: self.stake_pool_pubkey(),
            stake_pool_vault: self.stake_pool_vault_pubkey(),
            stake_account: self.stake_account_pubkey(recipient),
            staking_program: jet_staking::id(),
            token_program: spl_token::id(),
        };

        let mut instructions = self
            .rewards_program_client
            .request()
            .accounts(accounts)
            .args(jet_rewards::instruction::AirdropClaim {})
            .instructions()?;
        instructions.push(self.nonce_instruction());

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&recipient.pubkey()),
            &vec![recipient],
            self.env.get_recent_blockhash(),
        );
        Framework::process_tx_result(self.env.execute_transaction(transaction));

        Ok(())
    }
    pub fn mint_tokens(
        &mut self,
        mint: Pubkey,
        authority: &Keypair,
        destination: Pubkey,
        amount: u64,
    ) -> Result<(), Box<dyn Error>> {
        let instructions = vec![
            spl_token::instruction::mint_to(
                &spl_token::id(),
                &mint,
                &destination,
                &authority.pubkey(),
                &[],
                amount,
            )?,
            self.nonce_instruction(),
        ];
        Framework::process_tx_result(
            self.env
                .execute_as_transaction(&*instructions, &[authority]),
        );

        Ok(())
    }

    pub fn mint_vault_token(&mut self, user: &Keypair, amount: u64) -> Result<(), Box<dyn Error>> {
        let account = self
            .env
            .create_associated_token_account(user, self.vault_token_mint.pubkey());
        self.env.mint_tokens(
            self.vault_token_mint.pubkey(),
            &self.pool_authority,
            account,
            amount,
        );

        Ok(())
    }

    fn nonce_instruction(&mut self) -> Instruction {
        let instruction = Instruction::new_with_bytes(
            self.nop_program_pubkey,
            &self.tx_nonce.to_le_bytes(),
            vec![],
        );
        self.tx_nonce += 1;

        instruction
    }
}

pub fn clone_keypair(keypair: &Keypair) -> Keypair {
    Keypair::from_bytes(&keypair.to_bytes()).unwrap()
}

pub fn get_balance(
    test_env: &Framework,
    user: &Keypair,
    mint_pubkey: &Pubkey,
) -> Result<u64, Box<dyn Error>> {
    Ok(spl_token::state::Account::unpack(
        &test_env
            .env
            .get_account(get_associated_token_address(&user.pubkey(), mint_pubkey))
            .unwrap()
            .data,
    )?
    .amount)
}
