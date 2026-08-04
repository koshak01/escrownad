# Deploy — escrownad (forge-канон)

Prod path on foothold: `/work/production/rust/escrownad/`  
Bare git: `/work/git/rust/escrownad.git`

## Hyperion checklist

1. `git pull` on production worktree  
2. `cargo build --release --bins` (needs `../forge`)  
3. Seeds if fresh DB: `forge/docs/db_schema.sql` + `seeds/forge.sql` + deals  
4. Symlink prod configs:
   ```bash
   ln -sf deploy/etc/web.toml etc/web.toml
   ln -sf deploy/etc/ws.toml etc/ws.toml
   ```
5. Supervisor: `ln -s .../deploy/supervisor/escrownad.conf /etc/supervisor/conf.d/`  
6. Nginx: `escrownad.com.conf` + `wst.escrownad.com.conf` (+ SSL, DNS A for both hosts)  
7. `supervisorctl restart escrownad:*`

## Socks

| External (nginx) | IPC |
|---|---|
| `/tmp/escrownad_web.sock` | `/tmp/escrownad.database.sock` etc. |
| `/tmp/escrownad_ws.sock` | |

## Bins

`escrownad-database|notifier|ws|web|observer`

SSL: Let's Encrypt / CF — **not** skeleton certs in git.
