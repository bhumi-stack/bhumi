# Bhumi Relay Protocol — v2

Bhumi Relay Protocol — v2
Status: Draft
Audience: MCU devices (ESP32 class), community relays
Guarantees: Synchronous request-response (no buffering)

----

Based on a [chat with ChatGPT][chat]:

[chat]: https://chatgpt.com/share/6976fc45-9b48-8000-9dbc-5fd854807c7f

> Bhumi v2 — synchronous request-response protocol:
>
> - MCU-first
> - relay-assisted P2P
> - **synchronous delivery** (no buffering, recipient must be online)
> - no global registry
> - **sender identity hidden from relay** (inside encrypted payload)
> - single-use preimage-based DoS protection
> - **request-response in single round-trip** (ACK carries response + new preimage)
> - sender always informed of delivery outcome
> - relay QA
> - TLS transport

----

## 1. Design Principles

1. MCU safety > everything else
2. Identity = cryptographic key (`id52`)
3. Relays are dumb, replaceable, untrusted
4. No global truth, no registry, no DHT
5. All state is soft and recoverable
6. DoS must be stopped before MCU work
7. Failure degrades service, not security
8. No anonymity claims
9. **Synchronous delivery**: Relay does NOT buffer messages. SEND only succeeds
   if recipient is currently connected. Sender always learns the outcome.
10. **Preimage renewal**: On successful delivery, recipient returns a new
    preimage to sender, enabling continued communication.

----

## 2. Identity & Cryptography

### 2.1 Identity

- `id52` = Ed25519 public key (32 bytes, 52-character encoding format using
  [BASE32_DNSSEC])
- Private key never leaves device

[BASE32_DNSSEC]: https://docs.rs/data-encoding/latest/data_encoding/constant.BASE32_DNSSEC.html

### 2.2 Signatures

- Ed25519
- Used only for:
    - receive authentication
    - presence assertions

### 2.3 Encryption

- Payloads are end-to-end encrypted
- Relay treats payload as opaque bytes
- **Preimage as sender identifier**: Recipient maintains local mapping of
  `preimage → sender_pubkey`. When a message arrives, recipient looks up
  the preimage to identify the sender and retrieve their public key.
- Sender identity is NEVER transmitted — relay cannot learn who is sending

----

## 3. Transport

- TCP
- TLS (port 443)
- One connection = one role
- Relay sends `HELLO` immediately on accept

----

## 4. Framing

All messages are binary, length-prefixed.

```
FRAME {
    u16 type
    u32 length
    bytes[length] payload
}
```

Big-endian.  
Malformed frames MUST cause connection close.

----

## 5. Core Message Types

### 5.1 HELLO (relay → client)

```txt
type = 0x01

HELLO {
    u8 protocol_version // = 1
    u32 relay_nonce
    u32 max_payload_size
}
```

----

### 5.2 I_AM (client → relay)

Used when receiving messages. Registers identity, commits, and recent responses.

```txt
type = 0x02

I_AM {
    bytes[32] id52
    bytes[64] signature           // Sign(relay_nonce || id52)
    u16       commit_count
    bytes[32] commits[commit_count]
    u16       response_count      // recent responses to carry over
    RECENT_RESPONSE responses[response_count]
}

RECENT_RESPONSE {
    bytes[32] preimage
    u32       response_len
    bytes[response_len] response
}
```

Effects:

- Relay binds this TCP connection to id52
- Relay stores commits in VALID_COMMITS[id52]
- Relay stores recent_responses in global RESPONSE_CACHE
- Binding is forgotten on disconnect (but RESPONSE_CACHE survives with TTL)

----

### 5.3 SEND (client → relay)

Synchronous request — blocks until recipient responds or error.
**Preimage acts as idempotency key** — safe to retry on network failure.

```txt
type = 0x03

SEND {
    bytes[32] to_id52
    bytes[32] preimage
    u32       payload_len
    bytes[payload_len] payload   // encrypted for recipient
}
```

Relay behavior:

1. **Check response cache**: If `preimage ∈ RESPONSE_CACHE`:
   - Return SEND_RESULT(status=0, cached_response)
   - Evict from cache
   - (Recipient not bothered on retry)

2. Check `to_id52` is currently connected → if not, SEND_RESULT(status=1)

3. Compute `commit = H(preimage)`

4. Check `commit ∈ VALID_COMMITS[to_id52]` → if not, SEND_RESULT(status=2)

5. Remove commit (consume it, single-use)

6. Forward to recipient via DELIVER

7. Wait for ACK from recipient (with timeout)

8. **Cache response**: `RESPONSE_CACHE[preimage] = response` (with TTL)

9. Return SEND_RESULT(status=0, response) to sender

Relay MUST NOT:
- Buffer messages for offline recipients (but caches responses for retry)
- Learn or store sender identity

----

### 5.4 DELIVER (relay → client)

```txt
type = 0x04

DELIVER {
    u32 msg_id
    u32 payload_len
    bytes[payload_len] payload
}
```

----

### 5.5 ACK (client → relay)

Response to DELIVER. Contains encrypted response + new preimage for sender.

```txt
type = 0x05

ACK {
    u32 msg_id
    u32 payload_len
    bytes[payload_len] payload   // encrypted for sender
}
```

The payload is encrypted with sender's public key (looked up via preimage)
and contains:
- Response message
- New preimage for sender's next request

Relay forwards this payload to sender in SEND_RESULT — relay cannot read it.

----

### 5.6 KEEPALIVE

```
type = 0x06

KEEPALIVE {}
```

----

### 5.7 SEND_RESULT (relay → client)

Response to SEND — delivery outcome.

```txt
type = 0x07

SEND_RESULT {
    u8        status        // 0 = success, non-zero = error
    u32       payload_len
    bytes[payload_len] payload   // from recipient's ACK (encrypted), empty on error
}
```

Status codes:
- 0: Success — payload contains recipient's encrypted response
- 1: Recipient not connected to this relay
- 2: Invalid or already-used preimage
- 3: Recipient timeout (connected but didn't ACK)
- 4: Recipient disconnected during delivery

----

## 6. Send Permission Model (Core DoS Defense)

### 6.1 One-Time Preimage Capabilities

For inbound capacity control, each device maintains:

```
preimage = random 256 bits
commit   = H(preimage)
```

- `commit` is shared with relay (for validation)
- `preimage` is shared only with intended sender (out-of-band)
- Recipient stores: `preimage → sender_pubkey` (local lookup table)

**Preimage serves dual purpose:**
1. DoS protection (single-use capability to send)
2. Sender identification (recipient looks up who gave this preimage)

--- 

### 6.2 Relay State

For each id52, relay stores:

```
VALID_COMMITS[id52] = { COMMIT_1, COMMIT_2, ..., COMMIT_N }
```

For idempotent retry, relay maintains a **global** cache (not per id52):

```
RESPONSE_CACHE[preimage] = {
    response: bytes,      // encrypted response from recipient
    expires_at: timestamp // TTL, e.g., 5 minutes
}
```

**Response cache properties:**

- **Global**: Keyed by preimage alone, not by id52
- **Survives recipient disconnect**: If Alice goes offline, cache remains
- **TTL-based eviction**: Entries expire after TTL (e.g., 5 minutes), no explicit ACK
- **Populated from two sources**:
  1. When recipient ACKs a DELIVER
  2. When recipient connects and uploads recent responses (see 6.6)

**Why global cache works:**
- Preimage is unique and random (256 bits)
- Only the authorized sender knows the preimage
- Cache lookup doesn't need to know recipient identity

----

### 6.3 Consumption Semantics

- A commit is consumed immediately when a valid preimage is presented
- ACK is not required for protection
- Replay is impossible

This guarantees:

- Offline safety
- Single-use semantics
- Bounded inbound capacity

----

### 6.4 Startup / Relay Rotation

When a device connects:

```
I_AM + COMMIT_SET {
    commits[]
}
```

Relay replaces any previous commit set for that id52.  
Old commits naturally die.

----

### 6.5 Security Properties

- Leaked commits are useless (hash, not preimage)
- Relays cannot fabricate sends
- Relays cannot burn capacity (don't know preimages)
- Attacker must possess preimage to send
- Each preimage allows exactly one message (then consumed)
- **Relay cannot identify sender** (preimage only meaningful to recipient)
- **New preimage in response** enables continued conversation
- **Idempotent retry**: Same preimage returns cached response, no duplicate processing

----

### 6.6 Portable Response Cache (Recipient → New Relay)

When recipient (Alice) switches to a new relay, she carries recent responses.

**Recipient local state:**
```
LAST_RESPONSE[sender_pubkey] = {
    preimage: bytes[32],
    response: bytes,
    created_at: timestamp
}
```

Alice stores **only one response per sender** (the latest). If Bob sends 10 messages
in 5 minutes, Alice only keeps the most recent response to Bob. This bounds memory
on MCU devices.

**On I_AM to new relay:**
```
I_AM {
    id52
    signature
    commits[]
    recent_responses[]  // array of (preimage, response) pairs, one per sender
}
```

New relay populates its RESPONSE_CACHE from `recent_responses`.

**Benefits:**
- Bob can retry to **either** old relay or new relay
- Bob doesn't need to track which relay Alice is on
- Alice is never bothered twice for same request
- Memory bounded: one entry per active peer

----

## 7. Presence & id52 → Relay Mapping

### 7.1 Presence Assertion

To enable routing hints, a device periodically signs:

```
PRESENCE {
    id52
    relay_id
    issued_at
    ttl_secs // e.g. 300
}
signature = Sign(id52_priv, hash(PRESENCE))
```

Rules:

- Only id52 owner can assert presence
- TTL is authoritative
- Relays MUST NOT mint or extend presence

---- 

### 7.2 Relay Gossip

- Relays gossip PRESENCE to a small random set
- Cache in memory only
- Strict TTL enforcement

This provides **best-effort routing hints**, not guarantees.

---- 

## 8. Request-Response Flow (Bob → Alice)

### 8.1 Setup (out-of-band)

```
Alice generates preimage P1, stores: P1 → Bob's pubkey
Alice gives Bob: (Alice's id52, P1)
```

### 8.2 Message Flow

```
Sender (Bob)                Relay                    Recipient (Alice)
   │                          │                            │
   │  SEND(                   │                            │
   │    to=Alice,             │                            │
   │    preimage=P1,          │                            │
   │    encrypted_request     │                            │
   │  )                       │                            │
   │─────────────────────────►│                            │
   │                          │                            │
   │                          │  1. Check Alice connected  │
   │                          │  2. Verify H(P1) in commits│
   │                          │  3. Consume commit         │
   │                          │                            │
   │                          │  DELIVER(                  │
   │                          │    msg_id,                 │
   │                          │    encrypted_request       │
   │                          │  )                         │
   │                          │───────────────────────────►│
   │                          │                            │
   │                          │                            │ 4. Lookup P1 → Bob
   │                          │                            │ 5. Decrypt request
   │                          │                            │ 6. Process request
   │                          │                            │ 7. Generate new P2
   │                          │                            │ 8. Store P2 → Bob
   │                          │                            │ 9. Encrypt response+P2
   │                          │                            │
   │                          │  ACK(                      │
   │                          │    msg_id,                 │
   │                          │    encrypted(response, P2) │
   │                          │  )                         │
   │                          │◄───────────────────────────│
   │                          │                            │
   │  SEND_RESULT(            │                            │
   │    status=0,             │                            │
   │    encrypted(response,P2)│                            │
   │  )                       │                            │
   │◄─────────────────────────│                            │
   │                          │                            │
   │ 10. Decrypt response     │                            │
   │ 11. Extract P2 for next  │                            │
```

### 8.3 Retry Flow (Bob disconnected before receiving response)

```
First attempt — Bob disconnects:

Bob                         Relay                        Alice
 │  SEND(P1, payload) ─────►│                              │
 │                          │  consume commit H(P1) ✓      │
 │                          │  DELIVER ───────────────────►│
 │                          │◄─────────────── ACK(resp+P2) │
 │   ╳ disconnects          │                              │
 │                          │  cache[P1] = response        │
 │                          │  SEND_RESULT → fails         │

Retry — Bob reconnects to same relay:

Bob                         Relay
 │  SEND(P1, payload) ─────►│
 │                          │  P1 in RESPONSE_CACHE? Yes!
 │◄── SEND_RESULT(0, resp) ─│  (from cache, Alice not contacted)
 │                          │  evict cache[P1]
 │  Bob recovers P2 ✓       │
```

### 8.4 Error Flow

```
Sender (Bob)                Relay
   │                          │
   │  SEND(to=Alice, ...)     │
   │─────────────────────────►│
   │                          │  P1 not in cache
   │                          │  Alice not connected
   │  SEND_RESULT(            │
   │    status=1,             │
   │    payload=empty         │
   │  )                       │
   │◄─────────────────────────│
   │                          │
   │  Sender knows failure    │
   │  immediately, can retry  │
   │  later when Alice online │
```

### 8.5 Key Properties

- **Single round-trip**: Request and response in one exchange
- **Preimage renewal**: Each successful exchange yields next preimage
- **Sender hidden**: Relay never learns sender identity
- **No buffering**: Recipient must be online for first delivery
- **Sender feedback**: Sender always knows if delivery succeeded or failed
- **Idempotent retry**: Preimage is idempotency key; safe to retry on disconnect
- **Recipient protected**: Retry returns cached response, recipient not contacted twice

----

## 9. Relay Discovery

### 9.1 Bootstrap

Hardcoded seed relay list

---

### 9.2 Ongoing Discovery

Relay advertisements
Client caching and rotation
LAN discovery:
mDNS (_bhumi._tcp.local)
optional UDP broadcast
LAN relays are preferred but not trusted.

----

## 10. Relay QA (Device-Assisted)

**Principle**

Relay quality is assessed via **real client behavior**.

Properties

- Continuous
- Random
- Indistinguishable from real traffic
- Strictly rate-limited per device

**Mechanism**

- Relay may ask connected device to probe a candidate relay
- Device:
    - uses throwaway identity
    - sends message to itself
    - observes latency and correctness
- Device may refuse freely

QA results:

- Local only
- Never shared
- Influence relay preference only

----

## 11. Security Guarantees

**Guaranteed**

- MCU DoS bounded (preimage-based admission control)
- Single-use send permissions (replay impossible)
- Relay cannot spam device
- Relay cannot forge messages
- **Sender identity hidden from relay** (preimage identifies sender to recipient only)
- **Sender gets delivery feedback** (success/failure always reported)

**Not Guaranteed**

- Recipient anonymity (relay knows recipient id52)
- Metadata privacy (relay sees timing, message sizes)
- Offline delivery (recipient must be connected)
- Relay honesty
- Global reachability

----

## 12. Failure Model

- Recipient offline → SEND_RESULT(status=1), sender retries later
- Invalid preimage → SEND_RESULT(status=2), out-of-band recovery needed
- Recipient timeout → SEND_RESULT(status=3), sender can retry
- Sender disconnects before response → retry with same preimage, get cached response
- Relay disappears → sender reconnects to different relay (response lost if not cached)
- Preimage lost → out-of-band recovery needed

**Key principle**: Sender always learns the outcome. Retry is safe (idempotent).

---

## 13. Explicit Non-Goals

- DHT
- DNS identity mapping
- Proof-of-work
- Payments
- Reputation systems
- Perfect privacy
- Byzantine fault tolerance

----

## 14. One-Sentence Summary

> Bhumi v2 is an MCU-first, relay-assisted P2P protocol with synchronous
> request-response semantics, preimage-based DoS protection, and sender
> anonymity — where each successful exchange returns a new preimage for
> continued communication.

----
