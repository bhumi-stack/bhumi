# Bhumi Relay Protocol — v1

Bhumi Relay Protocol — v1  
Status: Draft  
Audience: MCU devices (ESP32 class), community relays  
Guarantees: Best-effort delivery only

----

Based on a [chat with ChatGPT][chat]:

[chat]: https://chatgpt.com/share/6976fc45-9b48-8000-9dbc-5fd854807c7f

> Below is a clean, internally consistent Bhumi v1 spec that incorporates
> everything we converged on and closes all the real holes we found:
>
> - MCU-first
> - relay-assisted P2P
> - best-effort delivery
> - no global registry
> - no sender identity
> - offline-safe, single-use send permissions
> - preimage-based DoS protection
> - relay discovery + relay QA
> - optional TLS camouflage
> - honest threat model

----

## 1. Design Principles

1. MCU safety > everything else
2. Identity = cryptographic key (`id52`)
3. Relays are dumb, replaceable, untrusted
4. No global truth, no registry, no DHT
5. All state is soft and recoverable
6. DoS must be stopped before MCU work
7. Failure degrades service, not security
8. No anonymity claims, no delivery guarantees

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
- Payload encryption format is out of scope

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

Used only when receiving messages.

```txt
type = 0x02

I_AM {
    bytes[32] id52
    bytes[64] signature // Sign(relay_nonce || id52)
}
```

Effects:

- Relay binds this TCP connection to id52
- Binding is forgotten on disconnect

----

### 5.3 SEND (client → relay)

Used to deliver a message via any relay.

```txt
type = 0x03

SEND {
    bytes[32] to_id52
    bytes[32] preimage    // y
    u32       payload_len
    bytes[payload_len] payload
}
```

Relay behavior:

1. Compute `x = H(preimage)`
2. Check `x ∈ VALID_COMMITS[to_id52]`
3. If not present → drop immediately
4. If present:
    - remove x (consume it)
    - forward message
5. Relay MUST NOT:
    - require sender identity
    - store sender identity

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

ACK is purely end-to-end semantics, not DoS defense.

```txt
type = 0x05

ACK {
    u32 msg_id
}
```

Used by applications for confirmation and logic.

----

### 5.6 KEEPALIVE

```
type = 0x06

KEEPALIVE {}
```

----

## 6. Send Permission Model (Core DoS Defense)

### 6.1 One-Time Preimage Capabilities

For inbound capacity control, each device maintains:

```
SECRET_i = random 256 bits
COMMIT_i = H(SECRET_i)
```

Only `COMMIT_i is` shared with relays  
`SECRET_i` is shared only with intended sender

--- 

### 6.2 Relay State

For each id52, relay stores:

```
VALID_COMMITS[id52] = { COMMIT_1, COMMIT_2, ..., COMMIT_N }
```

Properties:

- Soft-state
- Stored in RAM
- No expiry required
- No relay coordination required

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

- Leaked commits are useless
- Relays cannot fabricate sends
- Relays cannot burn capacity
- Attacker must possess secret to send
- Each secret allows exactly one message

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

## 8. Sending Flow (Bob → Alice)

Bob picks a random relay
Bob sends SEND(to_id52 = Alice, preimage)
Relay:
verifies commit
consumes it
routes message using presence hints
Alice receives message
Alice optionally ACKs
If anything fails:
message may be lost
Bob must wait for a new secret

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

- MCU DoS bounded
- Offline-safe inbound control
- Single-use send permissions
- Relay cannot spam device
- Relay cannot forge messages

**Not Guaranteed**

- Anonymity
- Metadata privacy
- Guaranteed delivery
- Relay honesty
- Global reachability

----

## 12. Failure Model

- Messages may be dropped
- Relays may lie or disappear
- Tokens may be lost
- Clients must retry
- Rotation is expected

Failures degrade service, not security.

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

> Bhumi v1 is a minimal, MCU-first, identity-based, relay-assisted P2P protocol
> that enforces explicit inbound capacity using one-time preimage capabilities,
> enabling offline-safe communication without global coordination or trusted
> infrastructure.

----
