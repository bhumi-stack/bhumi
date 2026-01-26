# Bhumi Device Protocol — v1

Status: Draft
Purpose: Device-to-device communication over Bhumi relays

----

## 1. Overview

This spec defines what devices send to each other **inside** the encrypted payloads
that flow through Bhumi relays. The relay protocol (bhumi-relay-protocol.md) handles
message transport; this spec handles message content.

**Relationship to Relay Protocol:**
```
┌─────────────────────────────────────────────────────────┐
│  Relay Protocol (transport)                             │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Device Protocol (this spec)                      │  │
│  │  - Invite tokens                                  │  │
│  │  - Initial handshake                              │  │
│  │  - Application messages                           │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

----

## 2. Invite Token

An invite token allows one device to contact another. It contains everything
needed for first contact.

### 2.1 Token Format

```
INVITE_TOKEN = base64url(
    id52[32]      // recipient's public key
    preimage[32]  // one-time preimage for first message
)
```

Total: 64 bytes → 86 characters base64url (no padding)

### 2.2 Token Generation

Recipient (Alice) generates an invite for a potential peer:

```
preimage = random(32 bytes)
commit = SHA256(preimage)
token = base64url(alice_id52 || preimage)
```

Alice stores locally:
```
invites[preimage] = {
    alias: "Bob",           // human-readable name
    created_at: timestamp,
    status: "pending",      // pending | paired | cancelled
    peer_id52: None,        // filled when peer responds
}
```

Alice registers `commit` with relay (in I_AM message).

### 2.3 Token Sharing

Alice shares the token with Bob via any channel:
- Email, SMS, QR code, NFC tap, verbal, etc.
- **One token = one peer** (don't reuse)

### 2.4 Token Usage

Bob receives token, parses it:
```
(alice_id52, preimage) = parse(token)
```

Bob generates his own preimage and stores in `pending_peers`:
```
my_preimage = random(32 bytes)

pending_peers[alice_id52] = {
    alias: "Alice",
    their_preimage: preimage,   // from invite, for HANDSHAKE_INIT
    my_preimage: my_preimage,   // for Alice to reply
}
```

Bob registers commit for `my_preimage` with relay.

----

## 3. Initial Handshake

The first message from Bob to Alice establishes bidirectional communication.

### 3.1 Handshake Flow

```
Bob                           Relay                         Alice
 │                              │                              │
 │  Parse invite token          │                              │
 │  → alice_id52, preimage      │                              │
 │                              │                              │
 │  SEND(                       │                              │
 │    to=alice_id52,            │                              │
 │    preimage,                 │                              │
 │    payload=HANDSHAKE_INIT    │                              │
 │  ) ─────────────────────────►│  DELIVER ───────────────────►│
 │                              │                              │
 │                              │                              │ Lookup preimage
 │                              │                              │ → invite for "Bob"
 │                              │                              │ Learn Bob's id52
 │                              │                              │ Generate preimage for Bob
 │                              │                              │
 │                              │  ACK(HANDSHAKE_COMPLETE) ◄───│
 │  SEND_RESULT ◄───────────────│                              │
 │                              │                              │
 │  Parse response              │                              │
 │  → new_preimage for Alice    │                              │
 │                              │                              │
 │  Both peers now paired       │                              │
```

### 3.2 HANDSHAKE_INIT (Bob → Alice)

First message Bob sends using the invite token.

```
HANDSHAKE_INIT {
    u8        msg_type = 0x01
    bytes[32] sender_id52       // Bob's public key
    bytes[32] preimage_for_peer // Preimage Alice can use to reply
    u16       relay_url_len
    bytes[relay_url_len] relay_url  // Bob's relay URL (e.g., "relay.example.com:443")
}
```

Including `relay_url` allows Alice to contact Bob directly without relay discovery.

The `preimage_for_peer` comes from `pending_peers[alice_id52].my_preimage` — Bob already
generated this when he received the invite token (see Section 2.4).

### 3.3 HANDSHAKE_COMPLETE (Alice → Bob)

Alice's response, sent in ACK payload.

```
HANDSHAKE_COMPLETE {
    u8        msg_type = 0x02
    u8        status           // 0 = accepted, non-zero = rejected
    bytes[32] preimage_for_peer // Preimage Bob can use for next message
    u16       relay_url_len
    bytes[relay_url_len] relay_url  // Alice's current relay
}
```

If accepted, Alice transitions state:
```
alias = invites[original_preimage].alias   // "Bob"
Remove invites[original_preimage]

peers[bob_id52] = {
    alias: alias,
    their_preimage: preimage_from_bob,     // from HANDSHAKE_INIT, for Alice to contact Bob
    issued_preimages: [preimage_for_peer], // Alice generates, sent in response
    last_known_relay: relay_url_from_bob,
}
```

### 3.4 After Handshake

When Bob receives HANDSHAKE_COMPLETE (status=0), he transitions state:
```
Remove pending_peers[alice_id52]

peers[alice_id52] = {
    alias: "Alice",
    their_preimage: preimage_from_alice,   // from HANDSHAKE_COMPLETE
    issued_preimages: [my_preimage],       // already registered with relay
    last_known_relay: relay_url_from_alice,
}
```

Both devices now have in `peers`:
- Each other's id52
- A preimage to send the next message (`their_preimage`)
- Preimages they issued for the peer to contact them (`issued_preimages`)
- Each other's relay URL

----

## 4. Application Messages

After handshake, devices exchange application messages.

### 4.1 MESSAGE (either direction)

```
MESSAGE {
    u8        msg_type = 0x10
    u8        content_type     // 0 = text, 1 = binary, 2 = command, ...
    u16       relay_url_len
    bytes[relay_url_len] relay_url  // sender's current relay
    u32       content_len
    bytes[content_len] content
}
```

Including `relay_url` in every message keeps peers updated on each other's relay.
If peer wants to initiate contact later, they start with this relay.
Falls back to relay discovery if stale.

### 4.2 MESSAGE_RESPONSE (in ACK)

```
MESSAGE_RESPONSE {
    u8        msg_type = 0x11
    u8        status           // 0 = ok, non-zero = error
    bytes[32] next_preimage    // for sender's next message
    u16       relay_url_len
    bytes[relay_url_len] relay_url  // responder's current relay
    u32       content_len
    bytes[content_len] content
}
```

**Every response includes:**
- New preimage for sender's next message
- Responder's current relay URL

----

## 5. Device State

Three data structures, no status fields. Data moves between structures as relationships progress.

### 5.1 Invites (I created, awaiting response)

Invites I created for potential peers. Keyed by preimage for fast lookup on incoming HANDSHAKE_INIT.

```
invites: Map<preimage, InviteRecord>

InviteRecord {
    alias: String,             // human-readable name for this invite
    created_at: Timestamp,
}
```

**Transitions:**
- On HANDSHAKE_INIT received: remove from `invites`, create entry in `peers`
- On cancel: remove from `invites`, revoke commit from relay

### 5.2 Pending Peers (I received invite, handshake not complete)

Peers I'm trying to connect to via their invite. I know their id52 but haven't completed handshake yet.

```
pending_peers: Map<id52, PendingPeerRecord>

PendingPeerRecord {
    alias: String,
    their_preimage: [u8; 32],  // from invite token, for HANDSHAKE_INIT
    my_preimage: [u8; 32],     // I generated, included in HANDSHAKE_INIT
    relay_url: String,         // where to reach them (may need discovery)
    created_at: Timestamp,
}
```

**Transitions:**
- On HANDSHAKE_COMPLETE received (status=0): remove from `pending_peers`, create entry in `peers`
- On HANDSHAKE_COMPLETE received (status≠0): remove from `pending_peers` (rejected)
- On timeout/give up: remove from `pending_peers`

### 5.3 Peers (established, bidirectional)

Fully paired peers. Both sides can send messages.

```
peers: Map<id52, PeerRecord>

PeerRecord {
    alias: String,
    last_known_relay: String,
    last_contacted: Timestamp,

    // Preimages I've issued to this peer (they use to contact me)
    issued_preimages: Vec<[u8; 32]>,

    // Preimage I use to contact them (they issued to me)
    their_preimage: [u8; 32],

    // Last response I sent to this peer (for relay cache portability)
    last_response: Option<ResponseCache>,
}

ResponseCache {
    preimage: [u8; 32],        // the preimage this response was for
    response: bytes,           // encrypted response
    created_at: Timestamp,
}
```

### 5.4 State Transitions

```
Alice creates invite for "Bob":
  preimage = random(32)
  invites[preimage] = { alias: "Bob", created_at: now }
  Register commit with relay

Bob receives invite token from Alice:
  (alice_id52, preimage) = parse(token)
  my_preimage = random(32)
  pending_peers[alice_id52] = {
      alias: "Alice",
      their_preimage: preimage,
      my_preimage: my_preimage,
      relay_url: discovered or default,
      created_at: now,
  }
  Register commit for my_preimage with relay

Bob sends HANDSHAKE_INIT:
  Use pending_peers[alice_id52].their_preimage
  Include my_preimage in payload

Alice receives HANDSHAKE_INIT (via invite):
  Lookup invites[preimage] → "Bob"
  bob_id52 = from payload
  new_preimage = random(32)

  Remove invites[preimage]
  peers[bob_id52] = {
      alias: "Bob",
      their_preimage: from HANDSHAKE_INIT,
      issued_preimages: [new_preimage],
      last_known_relay: from HANDSHAKE_INIT,
      ...
  }
  Send HANDSHAKE_COMPLETE with new_preimage

Bob receives HANDSHAKE_COMPLETE:
  Remove pending_peers[alice_id52]
  peers[alice_id52] = {
      alias: "Alice",
      their_preimage: from HANDSHAKE_COMPLETE,
      issued_preimages: [my_preimage],  // already registered
      last_known_relay: from HANDSHAKE_COMPLETE,
      ...
  }

Message received from established peer:
  Consume preimage from issued_preimages
  Add new preimage to issued_preimages (sent in response)
  Update last_response (for relay portability)
  Update last_contacted, last_known_relay
```

### 5.5 Relay Cache Portability

When connecting to new relay, include in I_AM:
```
recent_responses = peers.values()
    .filter(|p| p.last_response.is_some())
    .map(|p| (p.last_response.preimage, p.last_response.response))
```

This allows retry to work even if device switched relays.

----

## 6. Encryption

All payloads (HANDSHAKE_INIT, MESSAGE, etc.) are encrypted before sending.

### 6.1 Encryption Scheme

- **Key agreement**: X25519 (Curve25519 ECDH)
- **Encryption**: ChaCha20-Poly1305 (AEAD)
- **Key derivation**: HKDF-SHA256

### 6.2 Encrypted Payload Format

```
ENCRYPTED_PAYLOAD {
    bytes[32] ephemeral_pubkey  // sender's ephemeral X25519 public key
    bytes[24] nonce             // random nonce for ChaCha20-Poly1305
    bytes[16] tag               // authentication tag
    bytes[*]  ciphertext        // encrypted message
}
```

### 6.3 Encryption Flow

Sender (Bob) encrypting for recipient (Alice):

```
1. ephemeral_secret = random X25519 private key
2. ephemeral_pubkey = X25519_public(ephemeral_secret)
3. shared_secret = X25519(ephemeral_secret, alice_id52)
4. key = HKDF-SHA256(shared_secret, salt="bhumi-v1", info="encrypt")
5. nonce = random(24 bytes)
6. (ciphertext, tag) = ChaCha20-Poly1305-Encrypt(key, nonce, plaintext)
```

Recipient (Alice) decrypting:

```
1. shared_secret = X25519(alice_private_key, ephemeral_pubkey)
2. key = HKDF-SHA256(shared_secret, salt="bhumi-v1", info="encrypt")
3. plaintext = ChaCha20-Poly1305-Decrypt(key, nonce, ciphertext, tag)
```

----

## 7. Message Type Summary

| Type | Name | Direction | Description |
|------|------|-----------|-------------|
| 0x01 | HANDSHAKE_INIT | initiator → acceptor | First contact using invite |
| 0x02 | HANDSHAKE_COMPLETE | acceptor → initiator | Accept/reject + preimage |
| 0x10 | MESSAGE | either | Application message |
| 0x11 | MESSAGE_RESPONSE | responder → sender | Response + next preimage |

**All messages include `relay_url`** — peers opportunistically learn each other's current relay.

----

## 8. Example: Full Pairing Flow

```
1. Alice creates invite for "Bob"
   - Generates preimage P1
   - Stores: invites[P1] = { alias: "Bob", created_at: now }
   - Registers commit H(P1) with relay
   - Creates token: base64url(alice_id52 || P1)

2. Alice shares token with Bob (email)

3. Bob receives invite, creates pending peer
   - Parses token → alice_id52, P1
   - Generates preimage P2 (for Alice to reply)
   - Stores: pending_peers[alice_id52] = {
       alias: "Alice",
       their_preimage: P1,
       my_preimage: P2,
       ...
     }
   - Registers commit H(P2) with relay

4. Bob sends HANDSHAKE_INIT to Alice
   - SEND(to=alice_id52, preimage=P1, payload=encrypt(HANDSHAKE_INIT{bob_id52, P2, relay_url}))

5. Alice receives, processes
   - Lookup P1 → invites[P1] = "Bob"
   - Decrypt payload → learn bob_id52, P2, bob's relay
   - Generate P3 for Bob's next message
   - Remove invites[P1]
   - Create peers[bob_id52] = {
       alias: "Bob",
       their_preimage: P2,
       issued_preimages: [P3],
       last_known_relay: bob's relay,
       ...
     }
   - ACK with: encrypt(HANDSHAKE_COMPLETE{status=0, P3, relay_url})

6. Bob receives SEND_RESULT
   - Decrypt response → status=0, P3, alice's relay
   - Remove pending_peers[alice_id52]
   - Create peers[alice_id52] = {
       alias: "Alice",
       their_preimage: P3,
       issued_preimages: [P2],
       last_known_relay: alice's relay,
       ...
     }

7. Done! Alice and Bob are paired.
   - Alice can send to Bob using P2
   - Bob can send to Alice using P3
   - Both have each other's relay URLs
```

----

## 9. Security Considerations

- **Invite tokens are secrets**: Treat like passwords, don't reuse
- **One token = one peer**: If leaked, only one attacker can use it
- **Preimage rotation**: Every message gets a fresh preimage in response
- **Forward secrecy**: Ephemeral keys per message provide forward secrecy
- **No sender identity in transit**: Relay only sees recipient id52

----

## 10. Future Extensions

- **Device/Owner pairing**: Mobile app as controller for headless MCU
- **Group messaging**: Multiple devices in a group
- **Presence**: Online/offline status
- **Message types**: File transfer, streaming, etc.
