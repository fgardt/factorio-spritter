use std::{collections::BTreeMap, io::Write, path::Path};

#[derive(Debug, Clone)]
pub enum LuaValue {
    String(String),
    Float(f64),
    Int(i64),
    Bool(bool),
    Shift(f64, f64, usize),
    Array(Box<[LuaValue]>),
    Table(LuaOutput),
}

impl From<String> for LuaValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for LuaValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<f64> for LuaValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<f32> for LuaValue {
    fn from(value: f32) -> Self {
        Self::Float(value as f64)
    }
}

impl From<isize> for LuaValue {
    fn from(value: isize) -> Self {
        Self::Int(value as i64)
    }
}

impl From<i64> for LuaValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<i32> for LuaValue {
    fn from(value: i32) -> Self {
        Self::Int(value as i64)
    }
}

impl From<i16> for LuaValue {
    fn from(value: i16) -> Self {
        Self::Int(value as i64)
    }
}

impl From<i8> for LuaValue {
    fn from(value: i8) -> Self {
        Self::Int(value as i64)
    }
}

impl From<usize> for LuaValue {
    fn from(value: usize) -> Self {
        Self::Int(value as i64)
    }
}

impl From<u64> for LuaValue {
    fn from(value: u64) -> Self {
        Self::Int(value as i64)
    }
}

impl From<u32> for LuaValue {
    fn from(value: u32) -> Self {
        Self::Int(value as i64)
    }
}

impl From<u16> for LuaValue {
    fn from(value: u16) -> Self {
        Self::Int(value as i64)
    }
}

impl From<u8> for LuaValue {
    fn from(value: u8) -> Self {
        Self::Int(value as i64)
    }
}

impl From<bool> for LuaValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<(f64, f64, usize)> for LuaValue {
    fn from((shift_x, shift_y, res): (f64, f64, usize)) -> Self {
        Self::Shift(shift_x, shift_y, res)
    }
}

impl From<LuaOutput> for LuaValue {
    fn from(value: LuaOutput) -> Self {
        Self::Table(value)
    }
}

impl From<Box<[LuaOutput]>> for LuaValue {
    fn from(value: Box<[LuaOutput]>) -> Self {
        Self::Array(value.iter().map(|x| Self::Table(x.clone())).collect())
    }
}

impl LuaValue {
    fn gen_lua(&self, f: &mut dyn Write) -> std::io::Result<()> {
        match self {
            Self::String(value) => write!(f, "\"{value}\""),
            Self::Float(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Shift(x, y, res) => write!(f, "{{{x} / {res}, {y} / {res}}}"),
            Self::Array(arr) => {
                write!(f, "{{")?;
                for value in arr {
                    value.gen_lua(f)?;
                    write!(f, ",")?;
                }
                write!(f, "}}")
            }
            Self::Table(table) => table.gen_lua(f),
        }
    }

    fn gen_json(&self, f: &mut dyn Write) -> std::io::Result<()> {
        match self {
            Self::String(value) => write!(f, "\"{value}\""),
            Self::Float(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Shift(x, y, res) => {
                let x = x / *res as f64;
                let y = y / *res as f64;
                write!(f, "[{x},{y}]")
            }
            Self::Array(arr) => {
                write!(f, "[")?;
                let len = arr.len();
                for (i, value) in arr.iter().enumerate() {
                    value.gen_json(f)?;
                    if i < len - 1 {
                        write!(f, ",")?;
                    }
                }
                write!(f, "]")
            }
            Self::Table(table) => table.gen_json(f),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LuaOutput {
    map: BTreeMap<String, LuaValue>,
}

impl LuaOutput {
    pub const fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn set(mut self, key: impl AsRef<str>, value: impl Into<LuaValue>) -> Self {
        self.map.insert(key.as_ref().to_owned(), value.into());
        self
    }

    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;

        writeln!(
            file,
            "-- Generated by {} v{} - {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_REPOSITORY")
        )?;
        writeln!(file, "return {{")?;
        writeln!(
            file,
            "  [\"spritter\"] = {{ {}, {}, {} }},",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR"),
            env!("CARGO_PKG_VERSION_PATCH")
        )?;

        for (key, data) in &self.map {
            write!(file, "  [\"{key}\"] = ")?;
            data.gen_lua(&mut file)?;
            writeln!(file, ",")?;
        }

        writeln!(file, "}}")?;

        Ok(())
    }

    pub fn save_as_json(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;

        writeln!(file, "{{")?;
        writeln!(
            file,
            "  \"spritter\": [{},{},{}],",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR"),
            env!("CARGO_PKG_VERSION_PATCH")
        )?;

        let len = self.map.len();
        for (index, (key, data)) in self.map.iter().enumerate() {
            write!(file, "  \"{key}\": ")?;
            data.gen_json(&mut file)?;
            if index < len - 1 {
                writeln!(file, ",")?;
            } else {
                writeln!(file)?;
            }
        }

        writeln!(file, "}}")?;

        Ok(())
    }

    fn gen_lua(&self, f: &mut dyn Write) -> std::io::Result<()> {
        write!(f, "{{")?;

        for (key, data) in &self.map {
            write!(f, "[\"{key}\"] = ")?;
            data.gen_lua(f)?;
            write!(f, ",")?;
        }

        write!(f, "}}")?;

        Ok(())
    }

    fn gen_json(&self, f: &mut dyn Write) -> std::io::Result<()> {
        write!(f, "{{")?;

        let len = self.map.len();
        for (index, (key, data)) in self.map.iter().enumerate() {
            write!(f, "\"{key}\": ")?;
            data.gen_json(f)?;
            if index < len - 1 {
                write!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }

        write!(f, "}}")?;

        Ok(())
    }
}
