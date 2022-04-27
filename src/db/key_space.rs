use std::fmt::{Debug, Formatter};

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum KeySpace {
    META,
    HEADER,
    BODY,
    INDEX,
}

impl From<KeySpace> for u8 {
    fn from(k: KeySpace) -> Self {
        match k {
            KeySpace::META => 0,
            KeySpace::HEADER => 1,
            KeySpace::BODY => 2,
            KeySpace::INDEX => 3,
        }
    }
}

impl From<&KeySpace> for String {
    fn from(k: &KeySpace) -> Self {
        match k {
            KeySpace::META => "meta".to_string(),
            KeySpace::HEADER => "header".to_string(),
            KeySpace::BODY => "body".to_string(),
            KeySpace::INDEX => "index".to_string(),
        }
    }
}

impl Debug for KeySpace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self))
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn it_works() {
        assert_eq!(1 + 1, 2)
    }
}
