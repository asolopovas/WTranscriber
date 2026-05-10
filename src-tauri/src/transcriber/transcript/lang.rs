use super::Utterance;

pub(super) fn detect_script_lang(text: &str) -> Option<String> {
    let mut latin = 0usize;
    let mut cyrillic = 0usize;
    let mut cjk = 0usize;
    for c in text.chars() {
        let u = c as u32;
        if (0x0400..=0x04FF).contains(&u) || (0x0500..=0x052F).contains(&u) {
            cyrillic += 1;
        } else if (0x0041..=0x024F).contains(&u) {
            latin += 1;
        } else if (0x4E00..=0x9FFF).contains(&u)
            || (0x3040..=0x30FF).contains(&u)
            || (0xAC00..=0xD7AF).contains(&u)
        {
            cjk += 1;
        }
    }
    let total = latin + cyrillic + cjk;
    if total == 0 {
        return None;
    }
    if cyrillic * 2 >= total {
        Some("ru".into())
    } else if cjk * 2 >= total {
        Some("zh".into())
    } else if latin > 0 {
        Some("en".into())
    } else {
        None
    }
}

pub(super) fn resolve_language(meta_lang: &str, utts: &[Utterance]) -> String {
    let explicit = !meta_lang.is_empty() && meta_lang != "auto";
    if explicit {
        return meta_lang.into();
    }
    let mut seen: Vec<String> = Vec::new();
    for u in utts {
        if let Some(l) = &u.language
            && !seen.contains(l)
        {
            seen.push(l.clone());
        }
    }
    match seen.len() {
        0 => meta_lang.into(),
        1 => seen.into_iter().next().unwrap(),
        _ => seen.join(","),
    }
}
