# bankdemo

Bank demo gRPC server, migrated from the original Go implementation at
`internal/testing/cmd/bankdemo/` in the
[fullstorydev/grpcurl](https://github.com/fullstorydev/grpcurl) repository.
The Go version was originally created to showcase `grpcurl` at GopherCon 2018.

This Rust port is used to verify behavioral parity between the Go and Rust
`grpcurl` implementations with a feature-rich gRPC server that exercises:

- Unary RPCs (with request/response validation)
- Server streaming RPCs
- Bidirectional streaming RPCs
- Metadata-based authentication
- gRPC reflection (v1 and v1alpha)
- In-memory state with JSON persistence

## Services

### Bank (7 RPCs)

| RPC | Type | Description |
|-----|------|-------------|
| `OpenAccount` | Unary | Create a new account (checking, savings, money market, LOC, loan, equities) |
| `CloseAccount` | Unary | Close an account (must have zero balance) |
| `GetAccounts` | Unary | List all accounts for the authenticated customer |
| `GetTransactions` | Server streaming | Stream transactions filtered by date range |
| `Deposit` | Unary | Deposit funds (cash, check, ACH, wire) |
| `Withdraw` | Unary | Withdraw funds |
| `Transfer` | Unary | Transfer between local or external (ACH) accounts |

### Support (2 RPCs)

| RPC | Type | Description |
|-----|------|-------------|
| `ChatCustomer` | Bidi streaming | Customer-initiated chat sessions (init, message, hang up) |
| `ChatAgent` | Bidi streaming | Agent-facing chat (accept session from FIFO queue, message, leave) |

## Authentication

All RPCs require an `authorization` metadata header with format `token <id>`.

- **Customer tokens**: Any token not starting with "agent" (e.g., `token alice`)
- **Agent tokens**: Must start with "agent" (e.g., `token agent-bob`)

Bank RPCs require customer tokens. Support `ChatAgent` requires agent tokens;
`ChatCustomer` requires customer tokens.

## Usage

### Build

```bash
cargo build -p bankdemo
```

### Run

```bash
# Default: port 12345, data file accounts.json
cargo run -p bankdemo

# Custom port and data file
cargo run -p bankdemo -- -p 15000 -d /tmp/bank-data.json
```

### Interact with grpcurl

```bash
# List services
cargo run -p grpcurl -- -plaintext localhost:12345 list

# Describe the Bank service
cargo run -p grpcurl -- -plaintext localhost:12345 describe bank.Bank

# Open an account
cargo run -p grpcurl -- -plaintext \
  -H "authorization: token alice" \
  -d '{"initial_deposit_cents": 5000, "type": "CHECKING"}' \
  localhost:12345 bank.Bank/OpenAccount

# Get accounts
cargo run -p grpcurl -- -plaintext \
  -H "authorization: token alice" \
  localhost:12345 bank.Bank/GetAccounts

# Deposit funds
cargo run -p grpcurl -- -plaintext \
  -H "authorization: token alice" \
  -d '{"account_number": 1, "amount_cents": 1000, "source": "CASH"}' \
  localhost:12345 bank.Bank/Deposit
```
