//! FText serialization (Text.cpp / TextHistory.cpp).

use crate::errors::{Result, invalid};
use crate::package::Package;
use crate::reader::Reader;
use crate::versions::{custom, ue4};

#[derive(Debug, Clone)]
pub enum Text {
    /// Empty / culture invariant text.
    Invariant(String),
    Base { namespace: String, key: String, source: String },
    StringTable { table: String, key: String },
    NamedFormat { format: Box<Text>, args: Vec<(String, FormatArg)> },
    OrderedFormat { format: Box<Text>, args: Vec<FormatArg> },
    AsNumber { kind: &'static str, value: Box<FormatArg> },
    AsDateTime { kind: &'static str, ticks: i64 },
    Transform { source: Box<Text>, transform: u8 },
    Generator { type_id: String },
}

#[derive(Debug, Clone)]
pub enum FormatArg {
    Int(i64),
    UInt(u64),
    Float(f32),
    Double(f64),
    Text(Text),
    Gender(u8),
}

impl Text {
    /// Best-effort display string.
    pub fn display(&self) -> String {
        match self {
            Text::Invariant(s) => s.clone(),
            Text::Base { source, .. } => source.clone(),
            Text::StringTable { table, key } => format!("LOCTABLE({table}, {key})"),
            Text::NamedFormat { format, args } => {
                let mut s = format.display();
                for (k, v) in args {
                    s = s.replace(&format!("{{{k}}}"), &v.display());
                }
                s
            }
            Text::OrderedFormat { format, args } => {
                let mut s = format.display();
                for (i, v) in args.iter().enumerate() {
                    s = s.replace(&format!("{{{i}}}"), &v.display());
                }
                s
            }
            Text::AsNumber { value, .. } => value.display(),
            Text::AsDateTime { kind, ticks } => format!("{kind}({ticks})"),
            Text::Transform { source, transform } => {
                let s = source.display();
                match transform {
                    0 => s.to_lowercase(),
                    1 => s.to_uppercase(),
                    _ => s,
                }
            }
            Text::Generator { type_id } => format!("<TextGenerator {type_id}>"),
        }
    }

    pub fn key(&self) -> Option<(&str, &str)> {
        match self {
            Text::Base { namespace, key, .. } => Some((namespace, key)),
            _ => None,
        }
    }
}

impl FormatArg {
    pub fn display(&self) -> String {
        match self {
            FormatArg::Int(v) => v.to_string(),
            FormatArg::UInt(v) => v.to_string(),
            FormatArg::Float(v) => v.to_string(),
            FormatArg::Double(v) => v.to_string(),
            FormatArg::Text(t) => t.display(),
            FormatArg::Gender(g) => format!("Gender({g})"),
        }
    }
}

pub fn read_text(pkg: &Package, r: &mut Reader) -> Result<Text> {
    read_text_depth(pkg, r, 0)
}

fn read_text_depth(pkg: &Package, r: &mut Reader, depth: u32) -> Result<Text> {
    if depth > 16 {
        return invalid("FText nesting too deep");
    }
    if pkg.ue4() < ue4::FTEXT_HISTORY {
        let source = r.fstring()?;
        let (mut namespace, mut key) = (String::new(), String::new());
        if pkg.ue4() >= ue4::ADDED_NAMESPACE_AND_KEY_DATA_TO_FTEXT {
            namespace = r.fstring()?;
            key = r.fstring()?;
        }
        r.u32()?; // flags
        return Ok(Text::Base { namespace, key, source });
    }
    let _flags = r.u32()?;
    let history = r.i8()?;
    let t = match history {
        -1 => {
            let mut s = String::new();
            if pkg.custom_ver(custom::editor()) >= custom::ED_CULTURE_INVARIANT_TEXT_SERIALIZATION_KEY_STABILITY
                && r.bool32()?
            {
                s = r.fstring()?;
            }
            Text::Invariant(s)
        }
        0 => {
            let namespace = r.fstring()?;
            let key = r.fstring()?;
            let source = r.fstring()?;
            Text::Base { namespace, key, source }
        }
        1 => {
            let format = Box::new(read_text_depth(pkg, r, depth + 1)?);
            let n = r.count(5)?;
            let mut args = vec![];
            for _ in 0..n {
                let k = r.fstring()?;
                let v = read_format_arg(pkg, r, depth)?;
                args.push((k, v));
            }
            Text::NamedFormat { format, args }
        }
        2 => {
            let format = Box::new(read_text_depth(pkg, r, depth + 1)?);
            let n = r.count(1)?;
            let mut args = vec![];
            for _ in 0..n {
                args.push(read_format_arg(pkg, r, depth)?);
            }
            Text::OrderedFormat { format, args }
        }
        3 => {
            // ArgumentFormat: TArray<FFormatArgumentData>
            let format = Box::new(read_text_depth(pkg, r, depth + 1)?);
            let n = r.count(5)?;
            let mut args = vec![];
            for _ in 0..n {
                let k = r.fstring()?;
                let ty = r.u8()?;
                let v = read_format_arg_value(pkg, r, ty, depth, true)?;
                args.push((k, v));
            }
            Text::NamedFormat { format, args }
        }
        4 | 5 | 6 => {
            let kind = match history {
                4 => "AsNumber",
                5 => "AsPercent",
                _ => "AsCurrency",
            };
            if history == 6 && pkg.ue4() >= ue4::ADDED_CURRENCY_CODE_TO_FTEXT {
                r.fstring()?; // currency code
            }
            let value = read_format_arg(pkg, r, depth)?;
            if r.bool32()? {
                // FNumberFormattingOptions
                r.bool32()?; // AlwaysSign
                r.bool32()?; // UseGrouping
                r.i8()?; // RoundingMode
                r.i32()?;
                r.i32()?;
                r.i32()?;
                r.i32()?;
            }
            r.fstring()?; // target culture
            Text::AsNumber { kind, value: Box::new(value) }
        }
        7 => {
            let ticks = r.i64()?;
            r.i8()?; // date style
            if pkg.ue4() >= ue4::FTEXT_HISTORY_DATE_TIMEZONE {
                r.fstring()?;
            }
            r.fstring()?;
            Text::AsDateTime { kind: "AsDate", ticks }
        }
        8 => {
            let ticks = r.i64()?;
            r.i8()?;
            r.fstring()?;
            r.fstring()?;
            Text::AsDateTime { kind: "AsTime", ticks }
        }
        9 => {
            let ticks = r.i64()?;
            let date_style = r.i8()?;
            let time_style = r.i8()?;
            // EDateTimeStyle::Custom == 5 stores a pattern
            if date_style == 5 || time_style == 5 {
                r.fstring()?;
            }
            r.fstring()?;
            r.fstring()?;
            Text::AsDateTime { kind: "AsDateTime", ticks }
        }
        10 => {
            let source = Box::new(read_text_depth(pkg, r, depth + 1)?);
            let transform = r.u8()?;
            Text::Transform { source, transform }
        }
        11 => {
            let table = pkg.read_fname(r)?;
            let key = r.fstring()?;
            Text::StringTable { table, key }
        }
        12 => {
            let type_id = pkg.read_fname(r)?;
            if type_id != "None" {
                let n = r.count(1)?;
                r.skip(n)?;
            }
            Text::Generator { type_id }
        }
        other => return invalid(format!("unknown FText history type {other}")),
    };
    Ok(t)
}

fn read_format_arg(pkg: &Package, r: &mut Reader, depth: u32) -> Result<FormatArg> {
    let ty = r.u8()?;
    read_format_arg_value(pkg, r, ty, depth, false)
}

fn read_format_arg_value(pkg: &Package, r: &mut Reader, ty: u8, depth: u32, argument_data: bool) -> Result<FormatArg> {
    Ok(match ty {
        0 => {
            if argument_data && pkg.custom_ver(custom::editor()) < custom::ED_TEXT_FORMAT_ARGUMENT_DATA_IS_VARIANT {
                FormatArg::Int(r.i32()? as i64)
            } else {
                FormatArg::Int(r.i64()?)
            }
        }
        1 => FormatArg::UInt(r.u64()?),
        2 => FormatArg::Float(r.f32()?),
        3 => FormatArg::Double(r.f64()?),
        4 => FormatArg::Text(read_text_depth(pkg, r, depth + 1)?),
        5 => FormatArg::Gender(r.u8()?),
        other => return invalid(format!("unknown format argument type {other}")),
    })
}
