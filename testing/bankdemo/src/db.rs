use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use prost_types::Timestamp;
use serde::{Deserialize, Serialize};

use crate::pb;

// -- Serde-compatible DB types (prost_types::Timestamp doesn't impl serde) --

#[derive(Serialize, Deserialize)]
struct DbTimestamp {
    seconds: i64,
    nanos: i32,
}

impl From<&Timestamp> for DbTimestamp {
    fn from(ts: &Timestamp) -> Self {
        DbTimestamp {
            seconds: ts.seconds,
            nanos: ts.nanos,
        }
    }
}

impl From<&DbTimestamp> for Timestamp {
    fn from(ts: &DbTimestamp) -> Self {
        Timestamp {
            seconds: ts.seconds,
            nanos: ts.nanos,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DbTransaction {
    account_number: u64,
    seq_number: u64,
    date: DbTimestamp,
    amount_cents: i32,
    desc: String,
}

impl From<&pb::Transaction> for DbTransaction {
    fn from(t: &pb::Transaction) -> Self {
        DbTransaction {
            account_number: t.account_number,
            seq_number: t.seq_number,
            date: t
                .date
                .as_ref()
                .map(DbTimestamp::from)
                .unwrap_or(DbTimestamp {
                    seconds: 0,
                    nanos: 0,
                }),
            amount_cents: t.amount_cents,
            desc: t.desc.clone(),
        }
    }
}

impl From<&DbTransaction> for pb::Transaction {
    fn from(t: &DbTransaction) -> Self {
        pb::Transaction {
            account_number: t.account_number,
            seq_number: t.seq_number,
            date: Some(Timestamp::from(&t.date)),
            amount_cents: t.amount_cents,
            desc: t.desc.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DbAccount {
    account_number: u64,
    #[serde(rename = "type")]
    account_type: i32,
    balance_cents: i32,
    transactions: Vec<DbTransaction>,
}

#[derive(Serialize, Deserialize)]
struct DbAccounts {
    account_numbers_by_customer: HashMap<String, Vec<u64>>,
    accounts_by_number: HashMap<String, DbAccount>, // JSON keys must be strings
    account_numbers: Vec<u64>,
    customers: Vec<String>,
    last_account_num: u64,
}

// -- Runtime types --

pub struct Account {
    pub account_number: u64,
    pub account_type: i32,
    pub balance_cents: i32,
    pub transactions: Vec<pb::Transaction>,
}

impl Account {
    pub fn to_proto(&self) -> pb::Account {
        pb::Account {
            account_number: self.account_number,
            r#type: self.account_type,
            balance_cents: self.balance_cents,
        }
    }

    pub fn get_transactions(&self) -> Vec<pb::Transaction> {
        self.transactions.clone()
    }

    /// Create a new transaction, updating the balance. Returns the new balance.
    pub fn new_transaction(
        &mut self,
        amount_cents: i32,
        desc: String,
    ) -> Result<i32, tonic::Status> {
        let new_balance = self.balance_cents + amount_cents;
        if new_balance < 0 {
            return Err(tonic::Status::failed_precondition(format!(
                "insufficient funds: cannot withdraw {} when balance is {}",
                dollars(amount_cents),
                dollars(self.balance_cents)
            )));
        }
        self.balance_cents = new_balance;
        self.transactions.push(pb::Transaction {
            account_number: self.account_number,
            date: Some(now()),
            amount_cents,
            seq_number: self.transactions.len() as u64 + 1,
            desc,
        });
        Ok(new_balance)
    }
}

pub struct AccountStore {
    account_numbers_by_customer: HashMap<String, Vec<u64>>,
    accounts_by_number: HashMap<u64, Arc<RwLock<Account>>>,
    account_numbers: Vec<u64>,
    customers: Vec<String>,
    last_account_num: u64,
}

impl AccountStore {
    pub fn new() -> Self {
        AccountStore {
            account_numbers_by_customer: HashMap::new(),
            accounts_by_number: HashMap::new(),
            account_numbers: Vec::new(),
            customers: Vec::new(),
            last_account_num: 0,
        }
    }

    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let data = match std::fs::read_to_string(path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::new()),
            Err(e) => return Err(e.into()),
        };
        if data.trim().is_empty() {
            return Ok(Self::new());
        }
        let db: DbAccounts = serde_json::from_str(&data)?;
        let mut store = AccountStore {
            account_numbers_by_customer: db.account_numbers_by_customer,
            accounts_by_number: HashMap::new(),
            account_numbers: db.account_numbers,
            customers: db.customers,
            last_account_num: db.last_account_num,
        };
        for (key, db_acct) in db.accounts_by_number {
            let num: u64 = key.parse()?;
            let txns: Vec<pb::Transaction> =
                db_acct.transactions.iter().map(|t| t.into()).collect();
            let acct = Account {
                account_number: db_acct.account_number,
                account_type: db_acct.account_type,
                balance_cents: db_acct.balance_cents,
                transactions: txns,
            };
            store
                .accounts_by_number
                .insert(num, Arc::new(RwLock::new(acct)));
        }
        Ok(store)
    }

    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut accounts_by_number = HashMap::new();
        for (&num, acct_lock) in &self.accounts_by_number {
            let acct = acct_lock.read().unwrap();
            let db_acct = DbAccount {
                account_number: acct.account_number,
                account_type: acct.account_type,
                balance_cents: acct.balance_cents,
                transactions: acct.transactions.iter().map(|t| t.into()).collect(),
            };
            accounts_by_number.insert(num.to_string(), db_acct);
        }
        let db = DbAccounts {
            account_numbers_by_customer: self.account_numbers_by_customer.clone(),
            accounts_by_number,
            account_numbers: self.account_numbers.clone(),
            customers: self.customers.clone(),
            last_account_num: self.last_account_num,
        };
        let json = serde_json::to_string(&db)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn open_account(
        &mut self,
        customer: &str,
        account_type: i32,
        initial_balance_cents: i32,
    ) -> pb::Account {
        if !self.account_numbers_by_customer.contains_key(customer) {
            self.customers.push(customer.to_string());
        }
        let num = self.last_account_num + 1;
        self.last_account_num = num;
        self.account_numbers.push(num);

        let acct_nums = self
            .account_numbers_by_customer
            .entry(customer.to_string())
            .or_default();
        acct_nums.push(num);

        let acct = Account {
            account_number: num,
            account_type,
            balance_cents: initial_balance_cents,
            transactions: vec![pb::Transaction {
                account_number: num,
                seq_number: 1,
                date: Some(now()),
                amount_cents: initial_balance_cents,
                desc: "initial deposit".to_string(),
            }],
        };
        let proto = acct.to_proto();
        self.accounts_by_number
            .insert(num, Arc::new(RwLock::new(acct)));
        proto
    }

    pub fn close_account(
        &mut self,
        customer: &str,
        account_number: u64,
    ) -> Result<(), tonic::Status> {
        let acct_nums = self
            .account_numbers_by_customer
            .get(customer)
            .cloned()
            .unwrap_or_default();

        let found = acct_nums.iter().position(|&n| n == account_number);
        let found = match found {
            Some(i) => i,
            None => {
                return Err(tonic::Status::not_found(format!(
                    "you have no account numbered {}",
                    account_number
                )));
            }
        };

        let acct = self.accounts_by_number.get(&account_number).unwrap();
        let balance = acct.read().unwrap().balance_cents;
        if balance != 0 {
            return Err(tonic::Status::failed_precondition(format!(
                "account {} cannot be closed because it has a non-zero balance: {}",
                account_number,
                dollars(balance)
            )));
        }

        // Remove from account_numbers list
        if let Some(pos) = self
            .account_numbers
            .iter()
            .position(|&n| n == account_number)
        {
            self.account_numbers.remove(pos);
        }

        // Remove from customer's list
        let acct_nums = self.account_numbers_by_customer.get_mut(customer).unwrap();
        acct_nums.remove(found);

        self.accounts_by_number.remove(&account_number);
        Ok(())
    }

    pub fn get_account(
        &self,
        customer: &str,
        account_number: u64,
    ) -> Result<Arc<RwLock<Account>>, tonic::Status> {
        let acct_nums = self
            .account_numbers_by_customer
            .get(customer)
            .cloned()
            .unwrap_or_default();
        for num in acct_nums {
            if num == account_number {
                return Ok(Arc::clone(self.accounts_by_number.get(&num).unwrap()));
            }
        }
        Err(tonic::Status::not_found(format!(
            "you have no account numbered {}",
            account_number
        )))
    }

    pub fn get_all_accounts(&self, customer: &str) -> Vec<pb::Account> {
        let acct_nums = self
            .account_numbers_by_customer
            .get(customer)
            .cloned()
            .unwrap_or_default();
        let mut accounts = Vec::new();
        for num in acct_nums {
            if let Some(acct) = self.accounts_by_number.get(&num) {
                accounts.push(acct.read().unwrap().to_proto());
            }
        }
        accounts
    }

    /// Clone the store for safe serialization (no locks held during save).
    pub fn clone_for_save(&self) -> Self {
        let mut cloned = AccountStore {
            account_numbers_by_customer: self.account_numbers_by_customer.clone(),
            accounts_by_number: HashMap::new(),
            account_numbers: self.account_numbers.clone(),
            customers: self.customers.clone(),
            last_account_num: self.last_account_num,
        };
        for (&num, acct_lock) in &self.accounts_by_number {
            let acct = acct_lock.read().unwrap();
            let cloned_acct = Account {
                account_number: acct.account_number,
                account_type: acct.account_type,
                balance_cents: acct.balance_cents,
                transactions: acct.transactions.clone(),
            };
            cloned
                .accounts_by_number
                .insert(num, Arc::new(RwLock::new(cloned_acct)));
        }
        cloned
    }
}

fn now() -> Timestamp {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: dur.as_secs() as i64,
        nanos: dur.subsec_nanos() as i32,
    }
}

pub fn dollars(amount_cents: i32) -> String {
    format!("${:.2}", amount_cents as f64 / 100.0)
}
