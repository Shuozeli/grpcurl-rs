use std::sync::RwLock;

use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::auth;
use crate::db::{dollars, AccountStore};
use crate::pb;
use crate::pb::bank_server::Bank;

pub struct BankService {
    pub store: std::sync::Arc<RwLock<AccountStore>>,
}

#[tonic::async_trait]
impl Bank for BankService {
    async fn open_account(
        &self,
        request: Request<pb::OpenAccountRequest>,
    ) -> Result<Response<pb::Account>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let req = request.into_inner();
        let account_type = req.r#type;

        match account_type {
            // CHECKING=1, SAVING=2, MONEY_MARKET=3: allow deposit
            1..=3 => {
                if req.initial_deposit_cents < 0 {
                    return Err(Status::invalid_argument(format!(
                        "initial deposit amount cannot be negative: {}",
                        dollars(req.initial_deposit_cents)
                    )));
                }
            }
            // LINE_OF_CREDIT=4, LOAN=5, EQUITIES=6: must be zero
            4..=6 => {
                if req.initial_deposit_cents != 0 {
                    return Err(Status::invalid_argument(format!(
                        "initial deposit amount must be zero for account type {}: {}",
                        pb::account::Type::try_from(account_type)
                            .map(|t| format!("{:?}", t))
                            .unwrap_or_else(|_| account_type.to_string()),
                        dollars(req.initial_deposit_cents)
                    )));
                }
            }
            _ => {
                return Err(Status::invalid_argument(format!(
                    "invalid account type: {}",
                    account_type
                )));
            }
        }

        let mut store = self.store.write().unwrap();
        let acct = store.open_account(&cust, account_type, req.initial_deposit_cents);
        Ok(Response::new(acct))
    }

    async fn close_account(
        &self,
        request: Request<pb::CloseAccountRequest>,
    ) -> Result<Response<()>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let req = request.into_inner();
        let mut store = self.store.write().unwrap();
        store.close_account(&cust, req.account_number)?;
        Ok(Response::new(()))
    }

    async fn get_accounts(
        &self,
        request: Request<()>,
    ) -> Result<Response<pb::GetAccountsResponse>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let store = self.store.read().unwrap();
        let accounts = store.get_all_accounts(&cust);
        Ok(Response::new(pb::GetAccountsResponse { accounts }))
    }

    type GetTransactionsStream = ReceiverStream<Result<pb::Transaction, Status>>;

    async fn get_transactions(
        &self,
        request: Request<pb::GetTransactionsRequest>,
    ) -> Result<Response<Self::GetTransactionsStream>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let req = request.into_inner();
        let store = self.store.read().unwrap();
        let acct_lock = store.get_account(&cust, req.account_number)?;
        let acct = acct_lock.read().unwrap();
        let txns = acct.get_transactions();
        drop(acct);
        drop(store);

        // Parse start/end times
        let start_secs = req.start.as_ref().map(|ts| ts.seconds).unwrap_or(i64::MIN);
        let end_secs = req.end.as_ref().map(|ts| ts.seconds).unwrap_or(i64::MAX);

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            for txn in txns {
                let txn_secs = txn.date.as_ref().map(|ts| ts.seconds).unwrap_or(0);
                if txn_secs >= start_secs && txn_secs <= end_secs && tx.send(Ok(txn)).await.is_err()
                {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn deposit(
        &self,
        request: Request<pb::DepositRequest>,
    ) -> Result<Response<pb::BalanceResponse>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let req = request.into_inner();

        // Validate source
        match req.source {
            1..=4 => {} // CASH, CHECK, ACH, WIRE
            _ => {
                return Err(Status::invalid_argument(format!(
                    "unknown deposit source: {}",
                    req.source
                )));
            }
        }

        if req.amount_cents <= 0 {
            return Err(Status::invalid_argument(format!(
                "deposit amount cannot be non-positive: {}",
                dollars(req.amount_cents)
            )));
        }

        let source_name = pb::deposit_request::Source::try_from(req.source)
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|_| req.source.to_string());

        let desc = if req.desc.is_empty() {
            format!("{} deposit", source_name)
        } else {
            format!("{} deposit: {}", source_name, req.desc)
        };

        let store = self.store.read().unwrap();
        let acct_lock = store.get_account(&cust, req.account_number)?;
        drop(store);

        let mut acct = acct_lock.write().unwrap();
        let new_balance = acct.new_transaction(req.amount_cents, desc)?;

        Ok(Response::new(pb::BalanceResponse {
            account_number: req.account_number,
            balance_cents: new_balance,
        }))
    }

    async fn withdraw(
        &self,
        request: Request<pb::WithdrawRequest>,
    ) -> Result<Response<pb::BalanceResponse>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let req = request.into_inner();

        if req.amount_cents >= 0 {
            return Err(Status::invalid_argument(format!(
                "withdrawal amount cannot be non-negative: {}",
                dollars(req.amount_cents)
            )));
        }

        let store = self.store.read().unwrap();
        let acct_lock = store.get_account(&cust, req.account_number)?;
        drop(store);

        let mut acct = acct_lock.write().unwrap();
        let new_balance = acct.new_transaction(req.amount_cents, req.desc)?;

        Ok(Response::new(pb::BalanceResponse {
            account_number: req.account_number,
            balance_cents: new_balance,
        }))
    }

    async fn transfer(
        &self,
        request: Request<pb::TransferRequest>,
    ) -> Result<Response<pb::TransferResponse>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let req = request.into_inner();

        if req.amount_cents <= 0 {
            return Err(Status::invalid_argument(format!(
                "transfer amount cannot be non-positive: {}",
                dollars(req.amount_cents)
            )));
        }

        // Resolve source
        let (src_acct, src_desc) = match &req.source {
            Some(pb::transfer_request::Source::ExternalSource(ext)) => {
                let desc = format!(
                    "ACH {:09}:{:06}",
                    ext.ach_routing_number, ext.ach_account_number
                );
                if ext.ach_account_number == 0 || ext.ach_routing_number == 0 {
                    return Err(Status::invalid_argument(format!(
                        "external source routing and account numbers cannot be zero: {}",
                        desc
                    )));
                }
                (None, desc)
            }
            Some(pb::transfer_request::Source::SourceAccountNumber(num)) => {
                let desc = format!("account {:06}", num);
                let store = self.store.read().unwrap();
                let acct = store.get_account(&cust, *num)?;
                drop(store);
                (Some(acct), desc)
            }
            None => {
                return Err(Status::invalid_argument("source is required"));
            }
        };

        // Resolve destination
        let (dest_acct, dest_desc) = match &req.dest {
            Some(pb::transfer_request::Dest::ExternalDest(ext)) => {
                let desc = format!(
                    "ACH {:09}:{:06}",
                    ext.ach_routing_number, ext.ach_account_number
                );
                if ext.ach_account_number == 0 || ext.ach_routing_number == 0 {
                    return Err(Status::invalid_argument(format!(
                        "external source routing and account numbers cannot be zero: {}",
                        desc
                    )));
                }
                (None, desc)
            }
            Some(pb::transfer_request::Dest::DestAccountNumber(num)) => {
                let desc = format!("account {:06}", num);
                let store = self.store.read().unwrap();
                let acct = store.get_account(&cust, *num)?;
                drop(store);
                (Some(acct), desc)
            }
            None => {
                return Err(Status::invalid_argument("dest is required"));
            }
        };

        // Execute source withdrawal
        let mut src_balance: i32 = 0;
        let mut src_account_number: u64 = 0;
        if let Some(ref acct_lock) = src_acct {
            let withdraw_desc = if req.desc.is_empty() {
                format!("transfer to {}", dest_desc)
            } else {
                format!("transfer to {}: {}", dest_desc, req.desc)
            };
            let mut acct = acct_lock.write().unwrap();
            src_balance = acct.new_transaction(-req.amount_cents, withdraw_desc)?;
            src_account_number = acct.account_number;
        }

        // Execute destination deposit
        let mut dest_balance: i32 = 0;
        let mut dest_account_number: u64 = 0;
        if let Some(ref acct_lock) = dest_acct {
            let deposit_desc = if req.desc.is_empty() {
                format!("transfer from {}", src_desc)
            } else {
                format!("transfer from {}: {}", src_desc, req.desc)
            };
            let mut acct = acct_lock.write().unwrap();
            dest_balance = acct.new_transaction(req.amount_cents, deposit_desc)?;
            dest_account_number = acct.account_number;
        }

        Ok(Response::new(pb::TransferResponse {
            src_account_number,
            src_balance_cents: src_balance,
            dest_account_number,
            dest_balance_cents: dest_balance,
        }))
    }
}
