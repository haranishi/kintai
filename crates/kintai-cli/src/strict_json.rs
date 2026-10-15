//! 同じ名前の項目が2回あるオブジェクトを拒否して、JSON を読む（`docs/spec/validation.md` の手順1）。
//!
//! `serde_json::Value` へそのまま読むと、同じ名前の項目は黙って後の値に置き換わる。
//! 別実装と答えをそろえるため、ここで誤りにする。入れ子の深さの上限（128）は `serde_json` のものが効く。

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// 同じ名前の項目を許さずに読んだ JSON の値。
pub struct StrictValue(pub Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<StrictValue, D::Error> {
        deserializer.deserialize_any(StrictVisitor).map(StrictValue)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON の値")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    /// 小数（`420.0` や `1e2` など）。値は使わず、整数でないものとして後の検査で不正な入力にする。
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("数値が範囲外です"))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        StrictValue::deserialize(deserializer).map(|StrictValue(value)| value)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(StrictValue(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                return Err(de::Error::custom("同じ名前の項目が2回あります"));
            }
            let StrictValue(value) = map.next_value()?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::StrictValue;
    use serde_json::json;

    fn read(text: &str) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str::<StrictValue>(text).map(|StrictValue(value)| value)
    }

    #[test]
    fn reads_every_kind_of_value() {
        let value =
            read(r#"{"a": [1, -2, 18446744073709551615, 1.5, true, null, "x"], "b": {}}"#).unwrap();
        assert_eq!(
            value,
            json!({"a": [1, -2, 18_446_744_073_709_551_615_u64, 1.5, true, null, "x"], "b": {}})
        );
    }

    #[test]
    fn duplicate_names_are_rejected_at_any_depth() {
        assert!(read(r#"{"a": 1, "a": 1}"#).is_err());
        assert!(read(r#"{"x": [{"a": 1, "b": 2, "a": 3}]}"#).is_err());
        // エスケープを解いた後の名前で比べる。
        assert!(read(r#"{"a": 1, "a": 2}"#).is_err());
        assert!(read(r#"{"a": 1, "A": 2}"#).is_ok());
    }

    #[test]
    fn broken_json_and_trailing_text_are_rejected() {
        assert!(read("{").is_err());
        assert!(read("[1,]").is_err());
        assert!(read("{} x").is_err());
        assert!(read("1e400").is_err());
        let deep = "[".repeat(200) + &"]".repeat(200);
        assert!(read(&deep).is_err(), "入れ子の深さの上限");
    }
}
