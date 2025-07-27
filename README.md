# Single-Threaded Asynchronous TCP Broadcast (Rust/Tokio)

A minimal **single-threaded** TCP *broadcast server* in Rust. Multiple clients can connect; each line a client sends is **broadcast** to all other connected clients. The sender receives an ACK.

- **Runtime:** Tokio (`current_thread` flavor)
- **Threads:** 1 (no `Arc`, `Mutex`, or channels)
- **Client protocol:** newline-delimited text, works with `netcat`

---

## Protocol

On connect, the server greets the client: `LOGIN:{CLIENT_ID}`

For every line the client sends:
- Sender gets: `ACK:MESSAGE`
- All *other* clients get: `MESSAGE:{CLIENT_ID} {MESSAGE}`

**IDs:** CLIENT_ID is the connecting client’s ephemeral TCP **source port.**
**No history replay:** Clients only receive messages sent after they connect.

---

## Build & Run

Requires Rust stable. This repo pins Tokio to a stable 1.x that works broadly.

```bash
# Build & run (default port 8888)
cargo run --release

# Choose a custom port (e.g., 9000)
cargo run --release -- 9000
```

If you see Blocking waiting for file lock on package cache, stop background cargo processes (often rust-analyzer) and retry. See Troubleshooting below.

---

## Quick Test with netcat

In three terminals:

### Terminal A
```bash
nc localhost 8888
# -> LOGIN:<idA>
REQUEST
# -> ACK:MESSAGE
```

### Terminal B
```bash
nc localhost 8888
# -> LOGIN:<idB>
# -> MESSAGE:<idA> REQUEST
REPLY
# -> ACK:MESSAGE
# (A sees: MESSAGE:<idB> REPLY)
```

### Terminal C
```bash
nc localhost 8888
# -> LOGIN:<idC>
# -> MESSAGE:<idA> REQUEST
# -> MESSAGE:<idB> REPLY
```

---

## Design Notes
**Single-threaded async:**
Uses Tokio’s current_thread runtime and a single `tokio::select!` loop to multiplex:
- accepting incoming connections
- reading lines from all connected clients

**I/O model:**
- Each connection is split into read/write halves (`into_split`)
- Reads are line-oriented (`BufReader::lines()`), wrapped into a `LinesStream`
- All client streams are merged via a `StreamMap<client_id, LinesStream>`
- Writers are kept in a `HashMap<client_id, BufWriter<OwnedWriteHalf>>`

**Broadcast:**
For each incoming line, the server writes `MESSAGE:{id} …` to all *other* writers and `ACK:MESSAGE` to the sender. Failed writes/read errors remove that client.

## Assumptions
1.	**`CLIENT_ID`** = remote port of the TCP connection.
2.	**No replay/history:** messages are delivered only to currently connected clients.
3.	**Failure handling:** on read/write error, the client is dropped.
4.	**Line framing:** input and output are newline (\n) delimited.

## Troubleshooting
1. **“Blocking waiting for file lock on package cache”**
Another cargo (often spawned by your IDE’s rust-analyzer) is holding the lock.
```bash
ps aux | egrep 'cargo|rustc|rust-analyzer' | grep -v egrep
kill <PID>              # or close the IDE temporarily
cargo run --release
```
If needed: `CARGO_HOME="$(pwd)/.cargo-home" cargo run --release` to use a project-local cache.

2. **No LOGIN:… appears after connecting**
Ensure the server is running and force IPv4 (macOS may prefer IPv6 for localhost):
```bash
# Instead of "nc localhost 8888":
nc -v -4 127.0.0.1 8888
```

3. **Compiler version mismatch**
If a newer Tokio requires newer rustc, either: `rustup update stable` or or pin Tokio to a version supported by your toolchain in `Cargo.toml`.

## Project Layout
```tree
tcp-broadcast/
├─ Cargo.toml
└─ src/
   └─ main.rs
```