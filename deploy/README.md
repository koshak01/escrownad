# forge.foothold.me + forge-ws.foothold.me — deploy

Эталонный публичный экземпляр skeleton'а. На foothold.me крутится как
«живой документ» — любой Claude может зайти и посмотреть как работает
admin-UI, формы, login, WS-push HTML.

Также — **эталон deploy-flow для всех проектов на forge**. Когда mcpa9
(или любой следующий проект) копирует skeleton — он копирует **и эту
папку**, делая sed `forge` → `<новый-проект>` в supervisor/nginx-конфигах.

---

## Предусловия

- Hyperion (`html@foothold.me`, sudo).
- Postgres 18 на foothold.me. Эталонная схема — `forge/docs/db_schema.sql`.
- nginx с `--with-http_v3_module` (для HTTP/3 + QUIC).
- supervisor.
- Redis (`/var/run/redis/redis.sock`) — общий для всех проектов на foothold.me.
- DNS: `forge.foothold.me` и `forge-ws.foothold.me` → IP foothold'а (`5.75.210.111`).
  Cloudflare-запись делает Petr заранее.

---

## Шаги

### 1. Подтянуть код

```bash
cd /work/production/rust/forge
git pull origin master
cargo build --release -p forge-skeleton
```

Бинари окажутся в `/work/production/rust/forge/target/release/skeleton-{database,notifier,ws,web}`.

### 2. Создать БД

```bash
sudo -iu postgres psql
```

```sql
CREATE DATABASE "forge.foothold.me" OWNER html;
\c "forge.foothold.me"
\i /work/production/rust/forge/docs/db_schema.sql
\q
```

Мгновенно (template-клон), 0.05s. БД готова: ядерные таблицы + меню +
root-юзер Petr.

### 3. SSL — cert+key в репе проекта

Сертификаты живут **в самой forge-репе**, физическими файлами:

```
skeleton/deploy/nginx/ssl/cert.pem      (закоммичено)
skeleton/deploy/nginx/ssl/private.key   (закоммичено)
```

Они приезжают на прод через `git pull`. Nginx-конфиги ссылаются на
эти пути напрямую:

```
ssl_certificate     /work/production/rust/forge/skeleton/deploy/nginx/ssl/cert.pem;
ssl_certificate_key /work/production/rust/forge/skeleton/deploy/nginx/ssl/private.key;
```

**Никаких симлинков на `synapse/deploy/nginx/ssl/`** — это устаревшая
конвенция, не используется. Каждый проект (forge, mcpa9, sk8lls, …)
коммитит свой `deploy/nginx/ssl/{cert.pem,private.key}` в свою репу.

Получение wildcard `*.foothold.me` сертификата (если у проекта его ещё
нет) — задача Hyperion: certbot на foothold, потом копия cert+key в
`<project>/deploy/nginx/ssl/`, потом коммит в репу проекта.

### 4. Production etc — симлинки на `deploy/etc/` для ws и web

Локальные `etc/*.toml` нужны для dev-запуска через `cargo run`.
Production-overlay для `ws` и `web` лежит в `deploy/etc/` —
переключаем симлинками **только эти два**:

```bash
cd /work/production/rust/forge/skeleton
ln -sf ../deploy/etc/ws.toml  etc/ws.toml
ln -sf ../deploy/etc/web.toml etc/web.toml
```

Различия prod-overlay:
- `ws.toml` / `web.toml` — `mode=unix`, socket_path вместо `http_port`.

`etc/database.toml`, `etc/notifier.toml`, `etc/redis.toml` остаются
физическими файлами без overlay — там host=127.0.0.1 и БД-имя
одинаковы для dev и prod.

### 5. Supervisor — симлинк на конфиг

```bash
sudo ln -s /work/production/rust/forge/skeleton/deploy/supervisor/forge.conf \
           /etc/supervisor/conf.d/forge.conf
sudo supervisorctl reread
sudo supervisorctl update
```

Это создаст 4 программы (`forge_database`, `forge_notifier`, `forge_ws`,
`forge_web`) и группу `forge`. **Запускаем не сразу** — сначала nginx.

### 6. Nginx — симлинки на конфиги

```bash
sudo ln -s /work/production/rust/forge/skeleton/deploy/nginx/forge.foothold.me.conf \
           /etc/nginx/sites-enabled/forge.foothold.me.conf
sudo ln -s /work/production/rust/forge/skeleton/deploy/nginx/forge-ws.foothold.me.conf \
           /etc/nginx/sites-enabled/forge-ws.foothold.me.conf

sudo nginx -t        # проверка конфига
sudo systemctl reload nginx
```

### 7. Стартуем 4 сервиса

```bash
sudo supervisorctl start forge:*
sudo supervisorctl status forge:*
```

Все 4 должны быть в `RUNNING`. Проверяем логи:

```bash
tail -f /work/production/rust/log/forge_database.log
tail -f /work/production/rust/log/forge_ws.log
```

### 8. Smoke-test

```bash
curl -k https://forge.foothold.me/         # 200, главная skeleton'а
curl -k https://forge-ws.foothold.me/health # 200 OK
```

Открываем `https://forge.foothold.me/` в браузере — игнорируем SSL-warning
(self-signed). Должны увидеть главную с echo-формой.

---

## Обновление (новая версия forge или skeleton)

```bash
cd /work/production/rust/forge
git pull
cargo build --release -p forge-skeleton
sudo supervisorctl restart forge:*
```

Порядок restart — supervisor сам делает по priority (notifier → database →
ws → web). Downtime ~5 секунд.

---

## Удаление / переименование (если когда-нибудь)

```bash
sudo supervisorctl stop forge:*
sudo rm /etc/supervisor/conf.d/forge.conf
sudo supervisorctl reread && sudo supervisorctl update

sudo rm /etc/nginx/sites-enabled/forge.foothold.me.conf \
        /etc/nginx/sites-enabled/forge-ws.foothold.me.conf
sudo nginx -t && sudo systemctl reload nginx

# опционально:
sudo -iu postgres dropdb "forge.foothold.me"
```

---

## Каркас для следующих проектов

При создании нового проекта на forge (например `mcpa9`):

1. Скопировать `skeleton/` → `~/work/rust/mcpa9/`.
2. В скопированном sed: `skeleton` → `mcpa9` (имена бинарей в Cargo.toml,
   `init_with_file`, комментарии).
3. В скопированном sed: `forge` → `mcpa9` в:
   - `src/lib.rs sockets::*` (имена сокетов IPC),
   - `etc/*.toml` (БД-имя, socket_path),
   - `deploy/supervisor/*.conf` (программы),
   - `deploy/nginx/*.conf` (server_name, upstream, paths).
4. Hyperion поднимает по этому же чек-листу, заменяя `forge` на `mcpa9`.
