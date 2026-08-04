//! ЭТАЛОН доменной модели проекта — `demos`.
//!
//! Это НЕ бизнес-сущность, а учебный экспонат: показывает, как на forge
//! поднять admin-CRUD своей таблицы. Каждое поле демонстрирует одну
//! возможность стандартного списка/формы:
//!
//!   - `dmo_hash`      — логический уникум: `#[db(unique, hash)]`, sha256 бизнес-ключа
//!   - `dmo_code`      — уникальный код: text-фильтр (ILIKE) + сортируемая колонка
//!   - `dmo_title`     — заголовок: вторая сортируемая колонка + обычное поле формы
//!   - `dmo_note`      — свободный текст: «широкое» поле в форме
//!   - `dmo_amount`    — деньги `FixedN<8>` (или alias `Price`) → SQL `bigint` (НЕ numeric/f64)
//!   - `dmo_event_ts`  — доменное время `Timestamp` → SQL `timestamp` UTC (НЕ timestamptz)
//!   - `dmo_is_enable` — флаг: inline-toggle в строке + tribool-фильтр (Все/Да/Нет)
//!
//! Таймстампы (`dmo_dat` / `dmo_updated`) БД проставляет сама (`DEFAULT now()`),
//! в struct их нет — как у ядерных `Constant` / `Telegram`.
//!
//! Свою сущность делаешь по образцу. Полный рецепт —
//! `forge/docs/architecture/09_admin_lists.md`.
//!
//! ## ⚠️ КАНОН ТИПОВ — читай ПЕРЕД тем как писать свою таблицу
//!
//! Здесь forge местами идёт ПРОТИВ индустриального дефолта — где «как учит
//! Postgres-туториал» ведёт в яму. Это самые частые промахи новых проектов:
//!
//! | Величина            | forge-тип (Rust)        | SQL-тип           | ⚠️ НЕ бери         |
//! |---------------------|-------------------------|-------------------|--------------------|
//! | PK                  | `i64` + `#[db(skip_insert)]` | `bigint …IDENTITY` | не составной PK |
//! | Деньги / цена / объём | `FixedN<8>` или `Price`/`Volume` | `bigint`  | ❌ `numeric`, `f64`, СВОЙ alias |
//! | Процент             | `FixedN<4>` или `Percent` | `bigint`     | ❌ `numeric`, `f64`, СВОЙ alias |
//! | Время (сек)         | `Timestamp` (`forge_core`) | `timestamp` UTC | ❌ `timestamptz`   |
//! | Время (мс, тики)    | `TimestampMs`           | `timestamp` UTC   | ❌ `timestamptz`, `bigint` |
//! | Служебное созд/апдейт | — (нет в модели)      | `timestamp DEFAULT now()` | не держать в Rust |
//! | Логический уникум пары | `String` `#[db(unique, hash)]` | `varchar` (sha256) | не составной PK |
//!
//! Почему так: деньги — целочисленно (bigint), точная арифметика без float; время
//! — всё UTC как `timestamp`, без TZ-конверсий. Детали: `database/conventions/`
//! (`06_money.md`, `04_timestamps.md`). Копируешь эту модель — типы уже верные.
//!
//! ## Убрать / переименовать demo (ВСЕ точки проводки)
//!
//! Всё грепается на `demo`/`Demo`/`dmo_`:
//! `grep -rin demo skeleton/src skeleton/templates skeleton/seeds`. Точки:
//!   - `src/models/demo.rs` (этот файл) + `src/models/mod.rs` (mod + re-export)
//!   - `src/pages/admin/demos/` + `src/pages/admin/mod.rs` + `src/pages/mod.rs`
//!   - `src/ws_handlers/demos.rs` + `src/ws_handlers/mod.rs`
//!   - `src/lib.rs` — 4 варианта `DbCommand`, 2 варианта `DbResponse`,
//!     4 `client!`-метода, 3 `Box::new(...)` в `pages()`
//!   - `src/bin/database.rs` — 4 ветки `match` + `register_list_model("demos", …)`
//!   - `src/bin/ws.rs` — 2 объявления в макросе + 2 делегата + `use` params
//!   - `templates/admin/demos/{list,edit}.html.tera`
//!   - `seeds/demos.sql` + сама таблица: `DROP TABLE demos;`
//!
//! Удаление — снести всё перечисленное разом (частичное → build не соберётся:
//! `client!`/`match`/`pages()` сошлются на удалённые типы). Переименование в
//! свою сущность (`orders`/`ord_`) — те же точки, sed по `demo`/`Demo`/`dmo_`.

use forge_core::Timestamp;
use forge_core::hash::sha256_hex;
use forge_db::sqlx::{FromRow, PgPool};
use forge_db::{DbModel, ListFilter};
use forge_fixed_n::FixedN;
use serde::{Deserialize, Serialize};

/// Демо-сущность. `#[derive(DbModel)]` генерит `query()` / `insert` / `update`
/// / `delete` / `find_by_id` / `delete_by_id` по атрибуту `#[db(table, pk)]`.
#[derive(Debug, Clone, Default, FromRow, DbModel, Serialize, Deserialize)]
#[db(table = "demos", pk = "dmo_id")]
pub struct Demo {
    /// PK — serial, в INSERT не участвует.
    #[db(skip_insert)]
    pub dmo_id: i64,

    /// Хеш-колонка = **логический уникум** (эталон `#[db(unique, hash)]`).
    /// Когда логический ключ — не одна колонка, а комбинация (у Плутоса: пара
    /// рынков HL↔PM), НЕ делай составной PK. PK всегда `dmo_id bigserial`, а
    /// уникальность/дедуп — через хеш бизнес-ключа. `#[db(unique, hash)]` даёт
    /// `find_by_hash` / `delete_by_hash`. Значение генерит `save()` ниже
    /// (`sha256_hex(dmo_code)`) — SQL-тип `varchar`.
    #[db(unique, hash)]
    pub dmo_hash: String,

    /// Уникальный код (`ILIKE`-фильтр + сортировка + `<code>`-ячейка).
    #[db(unique)]
    pub dmo_code: Option<String>,

    /// Заголовок — вторая сортируемая колонка.
    pub dmo_title: Option<String>,

    /// Свободная заметка — демонстрирует «широкое» поле формы.
    pub dmo_note: Option<String>,

    /// Деньги/сумма. **SQL-тип `bigint`** (raw i64 = value × 10^8). ⚠️ НЕ
    /// `numeric`, НЕ `f64`: целочисленная арифметика без float-погрешности. Scale
    /// живёт только в Rust, в БД его нет. `DbModel` пишет/читает автоматом.
    ///
    /// Можно писать доменным алиасом — `Price`, `Volume`, `Percent`: с 2.86.0
    /// `DbModel` распознаёт их наравне с литеральным `FixedN<N>`, и для
    /// денежного поля алиас читается лучше. `Price` = `FixedN<8>`,
    /// `Volume` = `FixedN<8>`, `Percent` = `FixedN<4>`.
    ///
    /// ⚠️ Но только ЯДЕРНЫЕ алиасы. Собственный алиас проекта
    /// (`type Rub = FixedN<2>`) макрос не увидит: он матчит написанный токен,
    /// резолва алиасов у proc-macro нет. Для своей размерности пиши
    /// `FixedN<2>` литерально либо проси завести алиас в ядре.
    pub dmo_amount: FixedN<8>,

    /// Доменное время события — тип `Timestamp` (`forge_core`, UNIX-сек UTC).
    /// **SQL-тип `timestamp` (without time zone), UTC**. ⚠️ НЕ `timestamptz`:
    /// forge-контракт «всё в БД — UTC как timestamp», без TZ-конверсий Postgres.
    /// (Тик-данные в мс → `TimestampMs`.) Служебные `dmo_dat`/`dmo_updated` тут
    /// НЕ повторяем — их ставит БД (`DEFAULT now()`), в модели их нет.
    pub dmo_event_ts: Timestamp,

    /// Активность — inline-toggle в строке списка + tribool-фильтр.
    pub dmo_is_enable: bool,
}

/// Типизированный фильтр списка `/admin/demos/` (structure-driven UI, ADR-0010).
/// `#[derive(ListFilter)]` генерит `apply()` (наложение на `Demo::query()`) +
/// `to_fields()` (метаданные для `filter_accordion`). Гоняется по WS каноничным
/// msgpack, хранится в ядерной `filters.flt_params` — один источник правды.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ListFilter)]
#[list_filter(model = Demo)]
pub struct DemoListFilter {
    /// Код — `ILIKE %v%`.
    #[filter(text, col = "dmo_code", label = "Код")]
    pub code: Option<String>,
    /// Активность — Все / Да / Нет.
    #[filter(tribool, col = "dmo_is_enable", label = "Активно")]
    pub is_enable: Option<bool>,
}

impl Demo {
    /// save: `dmo_id == 0` → insert (и проставить новый id в self), иначе update.
    pub async fn save(&mut self, pool: &PgPool) -> forge_db::sqlx::Result<i64> {
        // Хеш — производная бизнес-ключа (dmo_code): эталон `#[db(unique, hash)]`.
        // Генерим ЗДЕСЬ (не в форме) — hash всегда консистентен ключу и уникален.
        self.dmo_hash = sha256_hex(self.dmo_code.as_deref().unwrap_or_default().as_bytes());
        if self.dmo_id > 0 {
            self.update(pool).await?;
            Ok(self.dmo_id)
        } else {
            let new_id = self.insert(pool).await?;
            self.dmo_id = new_id;
            Ok(new_id)
        }
    }
}
