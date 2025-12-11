# Fly.io Deployment Architecture for DailyReps Backup Server

## Important Note on Free Tier

Fly.io **no longer offers a free tier** to new customers as of 2024. New signups use **Pay As You Go** pricing. However, the costs for a small app like this are minimal (~$2-5/month).

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
│  │                     PRIVATE WIREGUARD MESH                        │  │
│  │                    (6PN internal networking)                      │  │
│  │                                                                   │  │
│  │   ┌─────────────────────┐         ┌─────────────────────┐        │  │
│  │   │   APP MACHINE       │         │   POSTGRES MACHINE  │        │  │
│  │   │   (Fly App)         │         │   (Fly Postgres)    │        │  │
│  │   │                     │         │                     │        │  │
│  │   │ ┌─────────────────┐ │  TCP    │ ┌─────────────────┐ │        │  │
│  │   │ │ dailyreps-      │ │ ──────► │ │ PostgreSQL 16   │ │        │  │
│  │   │ │ backup-server   │ │  5432   │ │                 │ │        │  │
│  │   │ │                 │ │         │ │                 │ │        │  │
│  │   │ │ (Rust/Axum)     │ │         │ └─────────────────┘ │        │  │
│  │   │ └─────────────────┘ │         │          │          │        │  │
│  │   │                     │         │          ▼          │        │  │
│  │   │   shared-cpu-1x     │         │ ┌─────────────────┐ │        │  │
│  │   │   256MB RAM         │         │ │  Volume (1GB+)  │ │        │  │
│  │   │                     │         │ │  Persistent     │ │        │  │
│  │   └─────────────────────┘         │ │  Storage        │ │        │  │
│  │                                   │ └─────────────────┘ │        │  │
│  │                                   │                     │        │  │
│  │                                   │   shared-cpu-1x     │        │  │
│  │                                   │   256MB RAM         │        │  │
│  │                                   └─────────────────────┘        │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Network Flow Detail

```
┌──────────────┐      ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│   DailyReps  │      │   Fly.io     │      │   App        │      │   Postgres   │
│   iOS App    │      │   Edge       │      │   Machine    │      │   Machine    │
└──────┬───────┘      └──────┬───────┘      └──────┬───────┘      └──────┬───────┘
       │                     │                     │                     │
       │  HTTPS POST         │                     │                     │
       │  /api/backup        │                     │                     │
       │────────────────────►│                     │                     │
       │                     │                     │                     │
       │                     │  HTTP (internal)    │                     │
       │                     │────────────────────►│                     │
       │                     │                     │                     │
       │                     │                     │  SQL Query          │
       │                     │                     │  (internal 6PN)     │
       │                     │                     │────────────────────►│
       │                     │                     │                     │
       │                     │                     │◄────────────────────│
       │                     │                     │  Result             │
       │                     │◄────────────────────│                     │
       │◄────────────────────│                     │                     │
       │  JSON Response      │                     │                     │
```

---

## What Lives Where

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        YOUR FLY.IO ORGANIZATION                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   APP 1: dailyreps-backup-server                                        │
│   ├── Type: Fly App (your Rust binary in Docker)                        │
│   ├── Machine: shared-cpu-1x, 256MB RAM                                 │
│   ├── Exposes: Port 8080 → HTTPS via Fly proxy                          │
│   ├── URL: https://dailyreps-backup-server.fly.dev                      │
│   └── Cost: ~$2/month (if running 24/7)                                 │
│                                                                         │
│   APP 2: dailyreps-backup-server-db  (auto-created by flyctl)           │
│   ├── Type: Fly Postgres (unmanaged)                                    │
│   ├── Machine: shared-cpu-1x, 256MB RAM                                 │
│   ├── Volume: 1GB persistent storage                                    │
│   ├── Internal only: postgres://...@dailyreps-backup-server-db.internal │
│   └── Cost: ~$2/month + $0.15/GB storage                                │
│                                                                         │
│   SECRETS (encrypted, injected as env vars):                            │
│   ├── DATABASE_URL                                                      │
│   ├── ALLOWED_ORIGINS                                                   │
│   └── APP_SIGNING_SECRET                                                │
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
       │                   │                   │                   │
       │◄──────────────────┴───────────────────┴───────────────────│
       │  Deployed! https://dailyreps-backup-server.fly.dev        │
```

---

## Estimated Monthly Costs (Pay As You Go)

```
┌────────────────────────────────────────────────────────────────┐
│  MINIMAL SETUP (Development)                                   │
├────────────────────────────────────────────────────────────────┤
│  App Machine (shared-cpu-1x, 256MB)      ~$1.94/month         │
│  Postgres Machine (shared-cpu-1x, 256MB) ~$1.94/month         │
│  Storage Volume (1GB)                    ~$0.15/month         │
│  Outbound bandwidth (first 100GB free)   ~$0.00               │
├────────────────────────────────────────────────────────────────┤
│  TOTAL                                   ~$4-5/month          │
└────────────────────────────────────────────────────────────────┘
```

---

## Key Points

1. **Two Fly Apps**: Your server and Postgres are separate Fly apps, each running in their own VM
2. **Private Network**: They communicate over Fly's private WireGuard mesh (6PN) - Postgres is NOT exposed to internet
3. **TLS Handled**: Fly terminates HTTPS at their edge, your app receives plain HTTP internally
4. **Single Region**: For cost savings, run both in one region (e.g., `iad` for US East)
5. **Postgres is Unmanaged**: You're responsible for backups, updates, and maintenance (though Fly provides daily snapshots)

---

## References

- [Fly.io Resource Pricing](https://fly.io/docs/about/pricing/)
- [Fly Postgres (Unmanaged) Docs](https://fly.io/docs/postgres/)
- [Free Postgres Blog Post](https://fly.io/blog/free-postgres/)
- [Managed Postgres (MPG) Docs](https://fly.io/docs/mpg/)
