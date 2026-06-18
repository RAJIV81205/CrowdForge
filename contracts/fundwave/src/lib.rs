//! FundWave — a minimal but complete crowdfunding contract for Stellar / Soroban.
//!
//! Lifecycle: Active -> Successful (goal met) | Failed (deadline passed).
//! Authorization via Address::require_auth. Storage keyed through DataKey enum.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
    String, Symbol, Vec,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    GoalMustBePositive = 1,
    DeadlineInPast = 2,
    AmountMustBePositive = 3,
    CampaignNotFound = 4,
    CampaignNotActive = 5,
    DeadlinePassed = 6,
    NotCreator = 7,
    NotFailed = 8,
    NothingToRefund = 9,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    NextId,
    Campaign(u64),
    Donor(u64, Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    Active,
    Successful,
    Failed,
    Withdrawn,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Campaign {
    pub id: u64,
    pub creator: Address,
    pub token: Address,
    pub goal: i128,
    pub raised: i128,
    pub deadline: u64,
    pub title: String,
    pub description: String,
    pub status: CampaignStatus,
}

#[contract]
pub struct Fundwave;

#[contractimpl]
impl Fundwave {
    pub fn init(env: Env) {
        if !env.storage().instance().has(&DataKey::NextId) {
            env.storage().instance().set(&DataKey::NextId, &1u64);
        }
    }

    pub fn create_campaign(
        env: Env,
        creator: Address,
        token: Address,
        goal: i128,
        deadline: u64,
        title: String,
        description: String,
    ) -> u64 {
        creator.require_auth();
        if goal <= 0 {
            panic_with_error!(&env, Error::GoalMustBePositive);
        }
        if deadline <= env.ledger().timestamp() {
            panic_with_error!(&env, Error::DeadlineInPast);
        }
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(1u64);
        env.storage()
            .instance()
            .set(&DataKey::NextId, &(next_id + 1));
        let campaign = Campaign {
            id: next_id,
            creator: creator.clone(),
            token,
            goal,
            raised: 0,
            deadline,
            title,
            description,
            status: CampaignStatus::Active,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(next_id), &campaign);
        env.storage().persistent().extend_ttl(
            &DataKey::Campaign(next_id),
            100_000,
            200_000,
        );
        env.events().publish(
            (Symbol::new(&env, "campaign_created"),),
            (next_id, creator, goal, deadline),
        );
        next_id
    }

    pub fn donate(env: Env, id: u64, donor: Address, amount: i128) {
        donor.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, Error::AmountMustBePositive);
        }
        let mut campaign = load_campaign(&env, id);
        if campaign.status != CampaignStatus::Active {
            panic_with_error!(&env, Error::CampaignNotActive);
        }
        if env.ledger().timestamp() > campaign.deadline {
            panic_with_error!(&env, Error::DeadlinePassed);
        }
        let token_client = token::Client::new(&env, &campaign.token);
        token_client.transfer(&donor, &env.current_contract_address(), &amount);
        campaign.raised = campaign.raised.checked_add(amount).expect("overflow");
        if campaign.raised >= campaign.goal {
            campaign.status = CampaignStatus::Successful;
        }
        env.storage().persistent().set(&DataKey::Campaign(id), &campaign);
        let key = DataKey::Donor(id, donor.clone());
        let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0i128);
        env.storage().persistent().set(&key, &(prev + amount));
        env.events().publish(
            (Symbol::new(&env, "donated"),),
            (id, donor, amount),
        );
    }

    pub fn get_campaign(env: Env, id: u64) -> Option<Campaign> {
        env.storage().persistent().get(&DataKey::Campaign(id))
    }

    pub fn list_campaigns(env: Env, from: u64, limit: u32) -> Vec<Campaign> {
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(1u64);
        let mut out: Vec<Campaign> = Vec::new(&env);
        let end = core::cmp::min(next_id, from.saturating_add(limit as u64));
        let mut i = from;
        while i < end {
            if let Some(c) = env
                .storage()
                .persistent()
                .get::<_, Campaign>(&DataKey::Campaign(i))
            {
                out.push_back(c);
            }
            i = i.saturating_add(1);
        }
        out
    }
}

fn load_campaign(env: &Env, id: u64) -> Campaign {
    env.storage()
        .persistent()
        .get(&DataKey::Campaign(id))
        .unwrap_or_else(|| panic_with_error!(env, Error::CampaignNotFound))
}

    pub fn finalize(env: Env, id: u64) {
        let mut campaign = load_campaign(&env, id);
        if campaign.status != CampaignStatus::Active {
            return;
        }
        if campaign.raised >= campaign.goal {
            campaign.status = CampaignStatus::Successful;
        } else if env.ledger().timestamp() > campaign.deadline {
            campaign.status = CampaignStatus::Failed;
        } else {
            return;
        }
        env.storage().persistent().set(&DataKey::Campaign(id), &campaign);
        env.events().publish(
            (Symbol::new(&env, "finalized"),),
            (id, campaign.status.clone()),
        );
    }

    pub fn withdraw(env: Env, id: u64) {
        let mut campaign = load_campaign(&env, id);
        campaign.creator.require_auth();
        if campaign.status != CampaignStatus::Successful {
            panic_with_error!(&env, Error::CampaignNotActive);
        }
        let token_client = token::Client::new(&env, &campaign.token);
        token_client.transfer(
            &env.current_contract_address(),
            &campaign.creator,
            &campaign.raised,
        );
        campaign.status = CampaignStatus::Withdrawn;
        env.storage().persistent().set(&DataKey::Campaign(id), &campaign);
        env.events().publish(
            (Symbol::new(&env, "withdrawn"),),
            (id, campaign.creator.clone(), campaign.raised),
        );
    }

    pub fn refund(env: Env, id: u64, donor: Address) {
        donor.require_auth();
        let campaign = load_campaign(&env, id);
        if campaign.status != CampaignStatus::Failed {
            panic_with_error!(&env, Error::NotFailed);
        }
        let key = DataKey::Donor(id, donor.clone());
        let amount: i128 = env.storage().persistent().get(&key).unwrap_or(0i128);
        if amount <= 0 {
            panic_with_error!(&env, Error::NothingToRefund);
        }
        let token_client = token::Client::new(&env, &campaign.token);
        token_client.transfer(&env.current_contract_address(), &donor, &amount);
        env.storage().persistent().set(&key, &0i128);
        env.events().publish(
            (Symbol::new(&env, "refunded"),),
            (id, donor, amount),
        );
    }
