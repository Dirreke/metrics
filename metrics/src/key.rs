use crate::{IntoLabels, Label, SharedString};
use alloc::{borrow::Cow, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    ops,
    slice::Iter,
};

/// Parts compromising a metric name.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum NameParts {
    /// Optimized path for inline storage.
    /// 
    /// This variant will be used primarily when the metric name is not scoped, and has not been
    /// modified via `append`/`prepend`.
    Inline([Option<SharedString>; 2]),
    /// Dynamic number of name parts.
    /// 
    /// If we do not have an open slot for appending/prepending, this variant will be used.
    Dynamic(Vec<SharedString>),
}

impl NameParts {
    /// Creates a [`NameParts`] from the given name.
    pub fn from_name<N: Into<SharedString>>(name: N) -> Self {
        NameParts::Inline([Some(name.into()), None])
    }

    /// Creates a [`NameParts`] from the given static name.
    pub const fn from_static_name(name: &'static str) -> Self {
        NameParts::Inline([Some(SharedString::const_str(name)), None])
    }

    /// Creates a [`NameParts`] from the given static name.
    pub const fn from_static_names(first: &'static str, second: &'static str) -> Self {
        NameParts::Inline([
            Some(SharedString::const_str(first)),
            Some(SharedString::const_str(second)),
        ])
    }

    /// Appends a name part.
    pub fn append<S: Into<SharedString>>(&mut self, part: S)  {
        match *self {
            NameParts::Inline(ref mut inner) => {
                if inner[1].is_none() {
                    // Open slot, so we can utilize it.
                    inner[1] = Some(part.into());
                } else {
                    // Have to spill over.
                    let mut parts = Vec::with_capacity(4);
                    parts.push(inner[0].clone().unwrap());
                    parts.push(inner[1].clone().unwrap());
                    parts.push(part.into());
                    *self = NameParts::Dynamic(parts);
                }
            },
            NameParts::Dynamic(ref mut parts) => {
                parts.push(part.into())
            },
        }
    }

    /// Prepends a name part.
    pub fn prepend<S: Into<SharedString>>(&mut self, part: S) {
        match *self {
            NameParts::Inline(ref mut inner) => {
                if inner[1].is_none() {
                    // Open slot, so we can utilize it.
                    inner[1] = inner[0].take();
                    inner[0] = Some(part.into());
                } else {
                    // Have to spill over.
                    let mut parts = Vec::with_capacity(4);
                    parts.push(part.into());
                    parts.push(inner[0].clone().unwrap());
                    parts.push(inner[1].clone().unwrap());
                    *self = NameParts::Dynamic(parts);
                }
            },
            NameParts::Dynamic(ref mut parts) => {
                parts.insert(0, part.into())
            },
        }
    }

    /// Gets a reference to the parts for this name.
    pub fn parts(&self) -> PartsIter {
        PartsIter::from(self)
    }
}

enum PartsIterInner<'a> {
    Inline(&'a [Option<SharedString>; 2]),
    Dynamic(Iter<'a, SharedString>),
}

/// Name parts iterator.
pub struct PartsIter<'a> {
    idx: usize,
    inner: PartsIterInner<'a>,
}

impl<'a> From<&'a NameParts> for PartsIter<'a> {
    fn from(parts: &'a NameParts) -> Self {
        let inner = match parts {
            NameParts::Inline(inner) => PartsIterInner::Inline(inner),
            NameParts::Dynamic(parts) => PartsIterInner::Dynamic(parts.iter()),
        };
        PartsIter { idx: 0, inner }
    }
}

impl<'a> Iterator for PartsIter<'a> {
    type Item = &'a SharedString;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner {
            PartsIterInner::Inline(inner) => {
                if self.idx > 1 { return None }
                
                let item = inner[self.idx].as_ref();
                self.idx += 1;
                item
            },
            PartsIterInner::Dynamic(ref mut iter) => iter.next(),
        }
    }
}

impl fmt::Display for NameParts {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NameParts::Inline(inner) => if inner[1].is_none() {
                write!(f, "{}", inner[0].as_ref().unwrap())
            } else {
                write!(f, "{}.{}", inner[0].as_ref().unwrap(), inner[1].as_ref().unwrap())
            },
            NameParts::Dynamic(parts) => {
                for (i, s) in parts.iter().enumerate() {
                    if i != parts.len() - 1 {
                        write!(f, "{}.", s)?;
                    } else {
                        write!(f, "{}", s)?;
                    }
                }
                Ok(())
            },
        }
    }
}

/// Inner representation of [`Key`].
///
/// While [`Key`] is the type that users will interact with via [`Recorder`][crate::Recorder`,
/// [`KeyData`] is responsible for the actual storage of the name and label data.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct KeyData {
    // TODO: once const slicing is possible on stable, we could likely use `beef` for both of these
    name_parts: Cow<'static, NameParts>,
    labels: Cow<'static, [Label]>,
}

impl KeyData {
    /// Creates a [`KeyData`] from a name.
    pub fn from_name<N>(name: N) -> Self
    where
        N: Into<SharedString>,
    {
        Self::from_parts(name, Vec::new())
    }

    /// Creates a [`KeyData`] from a name and vector of [`Label`]s.
    pub fn from_parts<N, L>(name: N, labels: L) -> Self
    where
        N: Into<SharedString>,
        L: IntoLabels,
    {
        Self {
            name_parts: Cow::Owned(NameParts::from_name(name)),
            labels: labels.into_labels().into(),
        }
    }

    /// Creates a [`KeyData`] from a static name.
    ///
    /// This function is `const`, so it can be used in a static context.
    pub const fn from_static_name(name_parts: &'static NameParts) -> Self {
        Self {
            name_parts: Cow::Borrowed(name_parts),
            labels: Cow::Owned(Vec::new()),
        }
    }

    /// Creates a [`KeyData`] from a static name and static set of labels.
    ///
    /// This function is `const`, so it can be used in a static context.
    pub const fn from_static_parts(name_parts: &'static NameParts, labels: &'static [Label]) -> Self {
        Self {
            name_parts: Cow::Borrowed(name_parts),
            labels: Cow::Borrowed(labels),
        }
    }

    /// Name parts of this key.
    pub fn name(&self) -> &NameParts {
        &self.name_parts
    }

    /// Labels of this key, if they exist.
    pub fn labels(&self) -> Iter<Label> {
        self.labels.iter()
    }

    /// Appends a part to the name,
    pub fn append_name<S: Into<SharedString>>(&mut self, part: S) {
        self.name_parts.to_mut().append(part)
    }

    /// Prepends a part to the name.
    pub fn prepend_name<S: Into<SharedString>>(&mut self, part: S) {
        self.name_parts.to_mut().prepend(part)
    }

    /// Consumes this [`Key`], returning the name parts and any labels.
    pub fn into_parts(self) -> (NameParts, Vec<Label>) {
        (self.name_parts.into_owned(), self.labels.into_owned())
    }

    /// Clones this [`Key`], and expands the existing set of labels.
    pub fn with_extra_labels(&self, extra_labels: Vec<Label>) -> Self {
        if extra_labels.is_empty() {
            return self.clone();
        }

        let name_parts = self.name_parts.clone();
        let mut labels = self.labels.clone().into_owned();
        labels.extend(extra_labels);

        Self {
            name_parts,
            labels: labels.into(),
        }
    }
}

impl fmt::Display for KeyData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.labels.is_empty() {
            write!(f, "KeyData({})", self.name_parts)
        } else {
            write!(f, "KeyData({}, [", self.name_parts)?;
            let mut first = true;
            for label in self.labels.as_ref() {
                if first {
                    write!(f, "{} = {}", label.0, label.1)?;
                    first = false;
                } else {
                    write!(f, ", {} = {}", label.0, label.1)?;
                }
            }
            write!(f, "])")
        }
    }
}

impl From<String> for KeyData {
    fn from(name: String) -> Self {
        Self::from_name(name)
    }
}

impl From<&'static str> for KeyData {
    fn from(name: &'static str) -> Self {
        Self::from_name(name)
    }
}

impl<N, L> From<(N, L)> for KeyData
where
    N: Into<SharedString>,
    L: IntoLabels,
{
    fn from(parts: (N, L)) -> Self {
        Self::from_parts(parts.0, parts.1)
    }
}

/// A metric identifier.
///
/// While [`KeyData`] holds the actual name and label data for a metric, [`Key`] works similar to
/// [`std::borrow::Cow`] in that we can either hold an owned version of the key data, or a static
/// reference to key data initialized elsewhere.
///
/// This allows for flexibility in the ways that [`KeyData`] can be passed around and reused, which
/// allows us to enable performance optimizations in specific circumstances.
#[derive(Debug, Clone)]
pub enum Key {
    /// A statically borrowed [`KeyData`].
    ///
    /// If you are capable of keeping a static [`KeyData`] around, this variant can be used to
    /// reduce allocations and improve performance.
    Borrowed(&'static KeyData),
    /// An owned [`KeyData`].
    ///
    /// Useful when you need to modify a borrowed [`KeyData`] in-flight, or when there's no way to
    /// keep around a static [`KeyData`] reference.
    Owned(KeyData),
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Key::Borrowed(inner) => inner.hash(state),
            Key::Owned(inner) => inner.hash(state),
        }
    }
}

impl Key {
    /// Converts any kind of [`Key`] into an owned [`KeyData`].
    ///
    /// If this key is owned, the value is returned as is, otherwise, the contents are cloned.
    pub fn into_owned(self) -> KeyData {
        match self {
            Self::Borrowed(val) => val.clone(),
            Self::Owned(val) => val,
        }
    }
}

impl ops::Deref for Key {
    type Target = KeyData;

    #[must_use]
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(val) => val,
            Self::Owned(val) => val,
        }
    }
}

impl AsRef<KeyData> for Key {
    #[must_use]
    fn as_ref(&self) -> &KeyData {
        match self {
            Self::Borrowed(val) => val,
            Self::Owned(val) => val,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Borrowed(val) => val.fmt(f),
            Self::Owned(val) => val.fmt(f),
        }
    }
}

impl From<KeyData> for Key {
    fn from(key_data: KeyData) -> Self {
        Self::Owned(key_data)
    }
}

impl From<&'static KeyData> for Key {
    fn from(key_data: &'static KeyData) -> Self {
        Self::Borrowed(key_data)
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, KeyData, NameParts};
    use crate::Label;
    use std::collections::HashMap;

    static BORROWED_NAME: NameParts = NameParts::from_static_name("name");
    static BORROWED_BASIC: KeyData = KeyData::from_static_name(&BORROWED_NAME);
    static LABELS: [Label; 1] = [Label::from_static_parts("key", "value")];
    static BORROWED_LABELS: KeyData = KeyData::from_static_parts(&BORROWED_NAME, &LABELS);

    #[test]
    fn test_keydata_eq_and_hash() {
        let mut keys = HashMap::new();

        let owned_basic = KeyData::from_name("name");
        assert_eq!(&owned_basic, &BORROWED_BASIC);

        let previous = keys.insert(owned_basic, 42);
        assert!(previous.is_none());

        let previous = keys.get(&BORROWED_BASIC);
        assert_eq!(previous, Some(&42));

        let labels = LABELS.to_vec();
        let owned_labels = KeyData::from_parts("name", labels);
        assert_eq!(&owned_labels, &BORROWED_LABELS);

        let previous = keys.insert(owned_labels, 43);
        assert!(previous.is_none());

        let previous = keys.get(&BORROWED_LABELS);
        assert_eq!(previous, Some(&43));
    }

    #[test]
    fn test_key_eq_and_hash() {
        let mut keys = HashMap::new();

        let owned_basic: Key = KeyData::from_name("name").into();
        let borrowed_basic: Key = Key::from(&BORROWED_BASIC);
        assert_eq!(owned_basic, borrowed_basic);

        let previous = keys.insert(owned_basic, 42);
        assert!(previous.is_none());

        let previous = keys.get(&borrowed_basic);
        assert_eq!(previous, Some(&42));

        let labels = LABELS.to_vec();
        let owned_labels = Key::from(KeyData::from_parts("name", labels));
        let borrowed_labels = Key::from(&BORROWED_LABELS);
        assert_eq!(owned_labels, borrowed_labels);

        let previous = keys.insert(owned_labels, 43);
        assert!(previous.is_none());

        let previous = keys.get(&borrowed_labels);
        assert_eq!(previous, Some(&43));
    }

    #[test]
    fn test_key_data_proper_display() {
        let key1 = KeyData::from_name("foobar");
        let result1 = key1.to_string();
        assert_eq!(result1, "KeyData(foobar)");

        let key2 = KeyData::from_parts("foobar", vec![Label::new("system", "http")]);
        let result2 = key2.to_string();
        assert_eq!(result2, "KeyData(foobar, [system = http])");

        let key3 = KeyData::from_parts(
            "foobar",
            vec![Label::new("system", "http"), Label::new("user", "joe")],
        );
        let result3 = key3.to_string();
        assert_eq!(result3, "KeyData(foobar, [system = http, user = joe])");

        let key4 = KeyData::from_parts(
            "foobar",
            vec![
                Label::new("black", "black"),
                Label::new("lives", "lives"),
                Label::new("matter", "matter"),
            ],
        );
        let result4 = key4.to_string();
        assert_eq!(
            result4,
            "KeyData(foobar, [black = black, lives = lives, matter = matter])"
        );
    }

    #[test]
    fn key_equality() {
        let owned_a = KeyData::from_name("a");
        let owned_b = KeyData::from_name("b");


        static A_NAME: NameParts = NameParts::from_static_name("a");
        static STATIC_A: KeyData = KeyData::from_static_name(&A_NAME);
        static B_NAME: NameParts = NameParts::from_static_name("b");
        static STATIC_B: KeyData = KeyData::from_static_name(&B_NAME);

        assert_eq!(Key::Owned(owned_a.clone()), Key::Owned(owned_a.clone()));
        assert_eq!(Key::Owned(owned_b.clone()), Key::Owned(owned_b.clone()));

        assert_eq!(Key::Borrowed(&STATIC_A), Key::Borrowed(&STATIC_A));
        assert_eq!(Key::Borrowed(&STATIC_B), Key::Borrowed(&STATIC_B));

        assert_eq!(Key::Owned(owned_a.clone()), Key::Borrowed(&STATIC_A));
        assert_eq!(Key::Owned(owned_b.clone()), Key::Borrowed(&STATIC_B));

        assert_eq!(Key::Borrowed(&STATIC_A), Key::Owned(owned_a.clone()));
        assert_eq!(Key::Borrowed(&STATIC_B), Key::Owned(owned_b.clone()));

        assert_ne!(Key::Owned(owned_a.clone()), Key::Owned(owned_b.clone()),);
        assert_ne!(Key::Borrowed(&STATIC_A), Key::Borrowed(&STATIC_B));
        assert_ne!(Key::Owned(owned_a.clone()), Key::Borrowed(&STATIC_B));
        assert_ne!(Key::Owned(owned_b.clone()), Key::Borrowed(&STATIC_A));
    }
}
