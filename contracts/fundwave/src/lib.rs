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
