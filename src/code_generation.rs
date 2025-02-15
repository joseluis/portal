use crate::gui::glsl::LibraryCode;
use crate::gui::material::Material;
use crate::gui::matrix::Matrix;
use crate::gui::object::Object;

use std::collections::BTreeMap;

use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct ErrId(pub usize);

// Used to find errors source
pub trait ErrorId {
    fn identifier(&self, pos: usize) -> ErrId;
}

impl ErrorId for Material {
    fn identifier(&self, pos: usize) -> ErrId {
        ErrId(1000 + pos)
    }
}

impl ErrorId for Object {
    fn identifier(&self, pos: usize) -> ErrId {
        ErrId(2000 + pos)
    }
}

impl ErrorId for LibraryCode {
    fn identifier(&self, pos: usize) -> ErrId {
        ErrId(3000 + pos)
    }
}

impl ErrorId for Matrix {
    fn identifier(&self, pos: usize) -> ErrId {
        ErrId(4000 + pos)
    }
}

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct LineNumbersByKey(pub BTreeMap<ErrId, Range<usize>>);

impl LineNumbersByKey {
    pub fn offset(&mut self, lines: usize) {
        self.0
            .iter_mut()
            .for_each(|(_, line)| *line = line.start + lines..line.end + lines);
    }

    pub fn add(&mut self, identifier: ErrId, lines: Range<usize>) {
        assert!(self.0.get(&identifier).is_none());
        self.0.insert(identifier, lines);
    }

    pub fn extend(&mut self, other: LineNumbersByKey) {
        for (k, v) in other.0 {
            self.add(k, v);
        }
    }

    // Returns identifier and local line position
    pub fn get_identifier(&self, line_no: usize) -> Option<(ErrId, usize)> {
        self.0
            .iter()
            .find(|(_, range)| range.contains(&line_no))
            .map(|(id, range)| (*id, line_no - range.start + 1))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StringStorage {
    pub storage: String,
    pub line_numbers: LineNumbersByKey,
    current_line_no: usize,
}

impl Default for StringStorage {
    fn default() -> Self {
        Self {
            storage: Default::default(),
            current_line_no: 1,
            line_numbers: Default::default(),
        }
    }
}

impl StringStorage {
    pub fn add_string<T: AsRef<str>>(&mut self, s: T) {
        self.current_line_no += s.as_ref().chars().filter(|c| *c == '\n').count();
        self.storage += s.as_ref();
    }

    pub fn add_identifier_string<T: AsRef<str>>(&mut self, identifier: ErrId, s: T) {
        let start = self.current_line_no;
        self.add_string(s);
        let end = self.current_line_no;
        self.line_numbers.add(identifier, start..end + 1);
    }

    pub fn add_string_storage(&mut self, mut other: StringStorage) {
        other.line_numbers.offset(self.current_line_no - 1);
        self.add_string(other.storage);
        self.line_numbers.extend(other.line_numbers);
    }
}

pub fn apply_template(
    template: &str,
    mut storages: BTreeMap<String, StringStorage>,
) -> StringStorage {
    let mut result = StringStorage::default();
    for (is_name, s) in template
        .split("//%")
        .enumerate()
        .map(|(pos, s)| (pos % 2 == 1, s))
    {
        if is_name {
            result.add_string_storage(storages.remove(s).expect(s));
        } else {
            result.add_string(s);
        }
    }
    result
}

#[cfg(test)]
mod tests_string_storage {
    use super::*;

    #[test]
    fn test() {
        let mut s1 = StringStorage::default();
        s1.add_string("1\n2\n3\n");
        s1.add_identifier_string(ErrId(1), "\n4\n5\n");

        assert_eq!(
            s1,
            StringStorage {
                storage: "1\n2\n3\n\n4\n5\n".to_owned(),
                current_line_no: 7,
                line_numbers: LineNumbersByKey(vec![(ErrId(1), 4..8)].into_iter().collect()),
            }
        );

        let mut s2 = StringStorage::default();
        s2.add_string("a\nb");
        s2.add_identifier_string(ErrId(2), "c\nd");

        assert_eq!(
            s2,
            StringStorage {
                storage: "a\nbc\nd".to_owned(),
                current_line_no: 3,
                line_numbers: LineNumbersByKey(vec![(ErrId(2), 2..4)].into_iter().collect()),
            }
        );

        let storages = vec![("s1".to_owned(), s1), ("s2".to_owned(), s2)]
            .into_iter()
            .collect::<BTreeMap<_, _>>();

        let s = apply_template("abc\n//%s2//%\n\ne\nf\n//%s1//%\n9", storages);

        assert_eq!(
            s,
            StringStorage {
                storage: "abc\na\nbc\nd\n\ne\nf\n1\n2\n3\n\n4\n5\n\n9".to_owned(),
                current_line_no: 15,
                line_numbers: LineNumbersByKey(
                    vec![(ErrId(1), 11..15), (ErrId(2), 3..5)]
                        .into_iter()
                        .collect()
                ),
            }
        );
    }
}
