//! Санация HTML из wysiwyg-редактора (Quill).
//!
//! Описание и условия сделки пользователь пишет в редакторе, то есть в базу
//! приходит HTML. Выводить его на странице сделки можно только через `| safe`,
//! а `| safe` над непроверенным вводом — это XSS: любой владелец кошелька
//! опубликовал бы лот со скриптом.
//!
//! Поэтому HTML чистится ЗДЕСЬ, на входе, по белому списку тегов. В шаблон
//! попадает уже безопасная разметка.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Теги, которые умеет ставить Quill и которые нужны для описания лота.
const ALLOWED_TAGS: &[&str] = &[
    "p", "br", "strong", "b", "em", "i", "u", "s", "ul", "ol", "li", "blockquote", "code", "pre",
    "h3", "h4", "a", "span",
];

static CLEANER: LazyLock<ammonia::Builder<'static>> = LazyLock::new(|| {
    let mut builder = ammonia::Builder::default();
    builder
        .tags(HashSet::from_iter(ALLOWED_TAGS.iter().copied()))
        // ссылки — только href, схемы ограничены ниже
        .link_rel(Some("noopener noreferrer nofollow"))
        .url_schemes(HashSet::from_iter(["http", "https", "mailto"]));
    builder
});

/// Чистит HTML из редактора по белому списку тегов.
///
/// Порядок работы:
/// 1. вырезает всё, чего нет в [`ALLOWED_TAGS`] (скрипты, iframe, обработчики);
/// 2. оставляет у ссылок только безопасные схемы (http/https/mailto);
/// 3. возвращает `None`, если после чистки не осталось текста — чтобы в базу
///    не попадала пустая разметка вида `<p><br></p>`.
///
/// # Параметры
/// * `raw` — исходный HTML из wysiwyg
///
/// # Возвращает
/// * `Option<String>` — очищенный HTML либо `None` для пустого содержимого
pub fn rich_text(raw: &str) -> Option<String> {
    let clean = CLEANER.clean(raw).to_string();
    if is_blank(&clean) { None } else { Some(clean) }
}

/// Плоский текст без разметки — для таблиц, заголовков вкладки, писем.
///
/// # Параметры
/// * `html` — размеченный текст
///
/// # Возвращает
/// * `String` — одна строка без тегов и лишних пробелов
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Содержимое пустое: ни текста, ни картинок — только пустые теги.
fn is_blank(html: &str) -> bool {
    plain_text(html).is_empty()
}
