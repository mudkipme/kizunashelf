use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub fn compare_string(a: &str, b: &str) -> std::cmp::Ordering {
    collator().compare(a, b).then_with(|| a.cmp(b))
}

pub fn compare_string_for_title_language(
    a: &str,
    b: &str,
    title_language: Option<&str>,
) -> std::cmp::Ordering {
    let Some(language) = title_language.and_then(normalize_locale_title_language) else {
        return compare_string(a, b);
    };

    let mut collators = locale_collators()
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if !collators.contains_key(&language) {
        if let Some(collator) = build_locale_collator(&language) {
            collators.insert(language.clone(), collator);
        }
    }

    collators
        .get(&language)
        .map(|collator| collator.compare(a, b).then_with(|| a.cmp(b)))
        .unwrap_or_else(|| compare_string(a, b))
}

fn collator() -> &'static icu_collator::CollatorBorrowed<'static> {
    use icu_collator::{options::CollatorOptions, CollatorBorrowed};
    use icu_locale_core::locale;
    static COLLATOR: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        CollatorBorrowed::try_new(locale!("zh-Hans-CN").into(), CollatorOptions::default()).unwrap()
    })
}

fn locale_collators() -> &'static Mutex<HashMap<String, icu_collator::CollatorBorrowed<'static>>> {
    static COLLATORS: OnceLock<Mutex<HashMap<String, icu_collator::CollatorBorrowed<'static>>>> =
        OnceLock::new();
    COLLATORS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn build_locale_collator(language: &str) -> Option<icu_collator::CollatorBorrowed<'static>> {
    use icu_collator::{options::CollatorOptions, CollatorBorrowed};
    use icu_locale_core::Locale;

    let locale = language.parse::<Locale>().ok()?;
    CollatorBorrowed::try_new(locale.into(), CollatorOptions::default()).ok()
}

fn normalize_locale_title_language(language: &str) -> Option<String> {
    let language = language.trim();
    let primary = language.split('-').next()?;
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|char| char.is_ascii_alphabetic())
    {
        return None;
    }
    Some(language.to_ascii_lowercase())
}

pub fn compare_optional_string(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => compare_string(a, b),
    }
}
