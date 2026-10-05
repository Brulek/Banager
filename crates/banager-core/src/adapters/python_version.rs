//! PEP 440 ordering for the legacy pipx registry check. Parsing is bounded
//! to u64 numeric components; an unrepresentable/invalid version is unknown.
use std::sync::OnceLock;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Bound<T> {
    Before,
    Value(T),
    After,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Local {
    Text(String),
    Number(u64),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    epoch: u64,
    release: Vec<u64>,
    pre: Bound<(u8, u64)>,
    post: Option<u64>,
    dev: Bound<u64>,
    local: Option<Vec<Local>>,
}

impl Version {
    fn parse(text: &str) -> Option<Self> {
        static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
        let re = PATTERN.get_or_init(|| regex::Regex::new(concat!(
            r"(?ix)^\s*v?(?:(?P<epoch>[0-9]+)!)?(?P<release>[0-9]+(?:\.[0-9]+)*)",
            r"(?:[-_.]?(?P<pre>a|b|c|rc|alpha|beta|pre|preview)[-_.]?(?P<pre_n>[0-9]+)?)?",
            r"(?:(?:-(?P<post_implicit>[0-9]+))|(?:[-_.]?(?P<post>post|rev|r)[-_.]?(?P<post_n>[0-9]+)?))?",
            r"(?:[-_.]?(?P<dev>dev)[-_.]?(?P<dev_n>[0-9]+)?)?",
            r"(?:\+(?P<local>[a-z0-9]+(?:[-_.][a-z0-9]+)*))?\s*$"
        )).expect("PEP 440 pattern"));
        let text = text.to_ascii_lowercase();
        let cap = re.captures(&text)?;
        let number = |name: &str| {
            cap.name(name)
                .map_or(Some(0), |m| m.as_str().parse::<u64>().ok())
        };
        let mut release: Vec<u64> = cap
            .name("release")?
            .as_str()
            .split('.')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        while release.last() == Some(&0) {
            release.pop();
        }
        let post = if let Some(m) = cap.name("post_implicit") {
            Some(m.as_str().parse().ok()?)
        } else if cap.name("post").is_some() {
            Some(number("post_n")?)
        } else {
            None
        };
        let dev = if cap.name("dev").is_some() {
            Bound::Value(number("dev_n")?)
        } else {
            Bound::After
        };
        let pre = if let Some(m) = cap.name("pre") {
            let phase = match m.as_str() {
                "a" | "alpha" => 0,
                "b" | "beta" => 1,
                _ => 2,
            };
            Bound::Value((phase, number("pre_n")?))
        } else if post.is_none() && matches!(dev, Bound::Value(_)) {
            Bound::Before
        } else {
            Bound::After
        };
        let local = cap
            .name("local")
            .map(|m| {
                m.as_str()
                    .split(['.', '-', '_'])
                    .map(|part| {
                        if part.bytes().all(|b| b.is_ascii_digit()) {
                            part.parse().ok().map(Local::Number)
                        } else {
                            Some(Local::Text(part.to_string()))
                        }
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .transpose_option()?;
        Some(Self {
            epoch: number("epoch")?,
            release,
            pre,
            post,
            dev,
            local,
        })
    }
}

// Option<Option<T>> is distinct from missing optional metadata: failed
// numeric parsing must reject the whole version.
trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(Some(v)) => Some(Some(v)),
            Some(None) => None,
        }
    }
}

pub(super) fn strictly_newer(latest: &str, current: &str) -> Option<bool> {
    let latest = Version::parse(latest)?;
    let current = Version::parse(current)?;
    Some(latest > current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_pep440_release_pre_post_dev_epoch_and_local_order() {
        let ordered = [
            "1.0.dev1",
            "1.0a1",
            "1.0b1.dev1",
            "1.0b1",
            "1.0rc1",
            "1.0",
            "1.0+abc",
            "1.0+abc.1",
            "1.0+1",
            "1.0.post1.dev1",
            "1.0.post1",
            "1.1",
            "1!0.1",
        ];
        for pair in ordered.windows(2) {
            assert_eq!(strictly_newer(pair[1], pair[0]), Some(true), "{pair:?}");
            assert_eq!(strictly_newer(pair[0], pair[1]), Some(false), "{pair:?}");
        }
        assert_eq!(strictly_newer("1.9", "2.0rc1"), Some(false));
        for (a, b) in [
            ("1.0", "1.0.0"),
            ("V01.0RC", "1.0preview0"),
            ("1.0-1", "1.0post1"),
            ("1.0+ABC-01", "1.0+abc.1"),
            ("1.0rev", "1.0.post0"),
        ] {
            assert_eq!(strictly_newer(a, b), Some(false));
            assert_eq!(strictly_newer(b, a), Some(false));
        }
        for invalid in [
            "latest",
            "1..2",
            "1.0+",
            "1.0rc-1x",
            "999999999999999999999999999",
        ] {
            assert_eq!(strictly_newer(invalid, "1.0"), None);
        }
    }
}
