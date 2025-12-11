# Fly.io Deployment Architecture for DailyReps Backup Server

## Important Note on Free Tier

Fly.io **no longer offers a free tier** to new customers as of 2024. New signups use **Pay As You Go** pricing. However, the costs for this app are minimal (~$2/month) thanks to using an embedded database.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           INTERNET                                       │
└─────────────────────────────────────────┬───────────────────────────────┘
                                          │
                                          │ HTTPS (TLS terminated by Fly)
                                          ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        FLY.IO EDGE / PROXY                              │
│                   (Anycast routing, TLS termination)                    │
└─────────────────────────────────────────┬───────────────────────────────┘
                                          │
                                          │ Internal Fly Network
                                          ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         FLY.IO REGION (e.g., iad)                       │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                                                                   │  │
│  │   ┌─────────────────────────────────────────────────────────┐     │  │
│  │   │                    APP MACHINE                          │     │  │
│  │   │                    (Fly App)                            │     │  │
│  │   │                                                         │     │  │
│  │   │   ┌─────────────────────────────────────────────────┐   │     │  │
│  │   │   │           dailyreps-backup-server               │   │     │  │
│  │   │   │                                                 │   │     │  │
│  │   │   │   ┌─────────────┐      ┌─────────────────────┐  │   │     │  │
│  │   │   │   │ Rust/Axum   │ ───► │  redb (embedded)    │  │   │     │  │
│  │   │   │   │ HTTP Server │      │  Key-Value Store    │  │   │     │  │
│  │   │   │   └─────────────┘      └─────────────────────┘  │   │     │  │
│  │   │   │                                  │              │   │     │  │
│  │   │   └──────────────────────────────────┼──────────────┘   │     │  │
│  │   │                                      │                  │     │  │
│  │   │      shared-cpu-1x                   ▼                  │     │  │
│  │   │      256MB RAM            ┌─────────────────────┐       │     │  │
│  │   │                           │  Volume (1GB)       │       │     │  │
│  │   │                           │  /data/backup.redb  │       │     │  │
│  │   │                           └─────────────────────┘       │     │  │
│  │   └─────────────────────────────────────────────────────────┘     │  │
│  │                                                                   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Network Flow Detail

```
┌──────────────┐      ┌──────────────┐      ┌──────────────────────────────┐
│   DailyReps  │      │   Fly.io     │      │        App Machine           │
│   iOS App    │      │   Edge       │      │  (Rust + embedded redb)      │
└──────┬───────┘      └──────┬───────┘      └──────────────┬───────────────┘
       │                     │                             │
       │  HTTPS POST         │                             │
       │  /api/backup        │                             │
       │────────────────────►│                             │
       │                     │                             │
       │                     │  HTTP (internal)            │
       │                     │────────────────────────────►│
       │                     │                             │
       │                     │                             │  Read/Write
       │                     │                             │  to redb file
       │                     │                             │  (in-process)
       │                     │                             │
       │                     │◄────────────────────────────│
       │◄────────────────────│                             │
       │  JSON Response      │                             │
```

---

## What Lives Where

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        YOUR FLY.IO ORGANIZATION                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   APP: dailyreps-backup-server                                          │
│   ├── Type: Fly App (Rust binary in Docker)                             │
│   ├── Machine: shared-cpu-1x, 256MB RAM                                 │
│   ├── Database: redb (embedded, in-process)                             │
│   ├── Volume: 1GB persistent storage at /data                           │
│   ├── Exposes: Port 8080 → HTTPS via Fly proxy                          │
│   ├── URL: https://dailyreps-backup-server.fly.dev                      │
│   └── Cost: ~$2/month                                                   │
│                                                                         │
│   SECRETS (encrypted, injected as env vars):                            │
│   ├── APP_SECRET_KEY (for HMAC signature verification)                  │
│   └── ALLOWED_ORIGINS (CORS whitelist)                                  │
│                                                                         │
│   VOLUME:                                                               │
│   ├── Name: dailyreps_data                                              │
│   ├── Mount: /data                                                      │
│   ├── Contains: backup.redb (all user data)                             │
│   └── Cost: ~$0.15/GB/month                                             │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Technology Stack

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          APPLICATION STACK                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   Web Framework:     Axum 0.8                                           │
│   Async Runtime:     Tokio 1.x                                          │
│   Middleware:        Tower 0.5 / Tower-HTTP 0.6                         │
│                                                                         │
│   Database:          redb 3 (embedded key-value store)                  │
│   Serialization:     bincode 2 (binary encoding for DB records)         │
│                      serde_json (API request/response)                  │
│                                                                         │
│   Security:          sha2 (SHA-256 hashing)                             │
│                      hmac (signature verification)                      │
│                                                                         │
│   Error Handling:    thiserror 2 / anyhow                               │
│   Logging:           tracing / tracing-subscriber                       │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Deployment Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Your Mac   │     │   fly.io    │     │   Docker    │     │  Fly.io     │
│  Terminal   │     │   CLI       │     │   Registry  │     │   Platform  │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │                   │
       │  fly deploy       │                   │                   │
       │──────────────────►│                   │                   │
       │                   │                   │                   │
       │                   │  Build Docker     │                   │
       │                   │  image from       │                   │
       │                   │  Dockerfile       │                   │
       │                   │──────────────────►│                   │
       │                   │                   │                   │
       │                   │                   │  Push image       │
       │                   │                   │──────────────────►│
       │                   │                   │                   │
       │                   │                   │     Deploy to     │
       │                   │                   │     Machine       │
       │                   │                   │     (attach vol)  │
       │                   │                   │                   │
       │◄──────────────────┴───────────────────┴───────────────────│
       │  Deployed! https://dailyreps-backup-server.fly.dev        │
```

---

## Estimated Monthly Costs (Pay As You Go)

```
┌────────────────────────────────────────────────────────────────┐
│  SINGLE-MACHINE SETUP (with embedded database)                 │
├────────────────────────────────────────────────────────────────┤
│  App Machine (shared-cpu-1x, 256MB)      ~$1.94/month         │
│  Storage Volume (1GB)                    ~$0.15/month         │
│  Outbound bandwidth (first 100GB free)   ~$0.00               │
├────────────────────────────────────────────────────────────────┤
│  TOTAL                                   ~$2/month            │
└────────────────────────────────────────────────────────────────┘
```

---

## Why Embedded Database (redb)?

| Aspect | PostgreSQL (old) | redb (current) |
|--------|------------------|----------------|
| **Cost** | ~$4/month (separate machine) | ~$2/month (single machine) |
| **Latency** | Network hop to DB | In-process, zero network |
| **Complexity** | 2 apps to manage | 1 app to manage |
| **Backups** | Fly snapshots | Fly volume snapshots |
| **Scaling** | Horizontal possible | Single-node only |

For a personal backup service with low traffic, the embedded approach is simpler and cheaper.

---

## Key Points

1. **Single Fly App**: Server and database run in the same process - no separate database machine needed
2. **Persistent Volume**: Data survives restarts via Fly volume mounted at `/data`
3. **TLS Handled**: Fly terminates HTTPS at their edge, your app receives plain HTTP internally
4. **Single Region**: Run in one region (e.g., `iad` for US East) for lowest latency to your location
5. **Zero-Knowledge**: All user data is encrypted client-side; server only stores encrypted blobs

---

## Environment Variables

```bash
# Required secrets (set via `fly secrets set`)
APP_SECRET_KEY=<random-string>      # Verifies HMAC signatures from app

# Optional
ALLOWED_ORIGINS=https://dailyreps.app,http://localhost:5173
DATABASE_PATH=/data/backup.redb     # Default path for redb file
```

---

## References

- [Fly.io Resource Pricing](https://fly.io/docs/about/pricing/)
- [Fly.io Volumes](https://fly.io/docs/volumes/)
- [redb - Rust Embedded Database](https://github.com/cberner/redb)
